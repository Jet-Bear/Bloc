// La ventana de Bloc: menús, archivo, deshacer, Markdown en vivo y barra de estado.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::System::Ole::{OleInitialize, CF_UNICODETEXT};
use windows_sys::Win32::System::Recovery::{RegisterApplicationRestart, RESTART_NO_CRASH, RESTART_NO_HANG};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, EM_CANUNDO, EM_SETREADONLY, EM_GETFIRSTVISIBLELINE, EM_LINEFROMCHAR, EM_LINEINDEX, EM_LINESCROLL, EM_UNDO, NMHDR, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT, ODS_NOACCEL, ODS_SELECTED};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::barras::{self, Barras, Boton, Icono};
use crate::config::Config;
use crate::fichero::{self, Aviso, Cod, Saltos};
use crate::historial::{self, Historial};
use crate::textos::t;
use crate::w::{con, enviar, mensaje, pulsada, seguro, ws};
use crate::nota::{self, Nota};
use crate::{buscar, dialogo, editor, md, re, tabla, tema};

const CR: u16 = 13;
// Por encima, un Markdown se abre como texto (con su barra de formato): pintar cada línea ya no compensa.
/// Las celdas de una tabla que se dibuja como tabla; más, se ve como texto.
const MAX_CELDAS: usize = 10_000;
const LIMITE_VIVO: usize = 1_000_000;

const ID_EDITOR: usize = 100;
const ID_BOTONES: usize = 101;
const ID_ESTADO: usize = 102;
const RELOJ_LISTA: usize = 1;

pub mod c {
    pub const NUEVO: u16 = 1000;
    pub const VENTANA: u16 = 1001;
    pub const ABRIR: u16 = 1002;
    pub const GUARDAR: u16 = 1003;
    pub const GUARDAR_COMO: u16 = 1004;
    pub const IMPRIMIR: u16 = 1005;
    pub const SALIR: u16 = 1006;
    pub const COPIA: u16 = 1007;
    pub const ELEGIR: u16 = 1008;
    pub const INFORMAR: u16 = 1009;
    pub const DESHACER: u16 = 1010;
    pub const REHACER: u16 = 1011;
    pub const CORTAR: u16 = 1012;
    pub const COPIAR: u16 = 1013;
    pub const PEGAR: u16 = 1014;
    pub const ELIMINAR: u16 = 1015;
    pub const TODO: u16 = 1016;
    pub const BUSCAR: u16 = 1017;
    pub const SIGUIENTE: u16 = 1018;
    pub const ANTERIOR: u16 = 1019;
    pub const REEMPLAZAR: u16 = 1020;
    pub const IR: u16 = 1021;
    pub const HORA: u16 = 1022;
    pub const PEGAR_TEXTO: u16 = 1023;
    pub const NEGRITA: u16 = 1030;
    pub const CURSIVA: u16 = 1031;
    pub const TACHADO: u16 = 1032;
    pub const CODIGO: u16 = 1033;
    pub const ENLACE: u16 = 1034;
    pub const TITULO1: u16 = 1035;
    pub const TITULO2: u16 = 1036;
    pub const TITULO3: u16 = 1037;
    pub const LISTA: u16 = 1038;
    pub const NUMERADA: u16 = 1039;
    pub const CASILLAS: u16 = 1040;
    pub const CITA: u16 = 1041;
    pub const TABLA: u16 = 1042;
    pub const ALINEAR: u16 = 1043;
    pub const AJUSTE: u16 = 1050;
    pub const BARRA: u16 = 1051;
    pub const NOCHE: u16 = 1052;
    pub const CORRECTOR: u16 = 1053;
    pub const LETRA: u16 = 1054;
    pub const ACERCAR: u16 = 1055;
    pub const ALEJAR: u16 = 1056;
    pub const NORMAL: u16 = 1057;
    pub const MARCAS: u16 = 1058;
    pub const NUEVOS_MD: u16 = 1059;
    pub const COD: u16 = 1060; // UTF-8, UTF-8 con BOM, UTF-16 LE, UTF-16 BE, ANSI
    pub const CONTRARIO: u16 = 1069;
    pub const SALTOS: u16 = 1070; // CRLF, LF, CR
    pub const MARGENES: u16 = 1080; // estrechos, normales, anchos, muy anchos
    pub const RECIENTE: u16 = 1090; // los 8 de Archivo › Recientes
    pub const BORRAR_RECIENTES: u16 = 1098;
    pub const NOTA: u16 = 1099;
}

// Los botones de los avisos de cerrar y de la nota.
const GUARDAR: i32 = 6;
const NO_GUARDAR: i32 = 7;
const SEGUIR: i32 = 10;
const ANADIR: i32 = 11;
const SUSTITUIR: i32 = 12;
const OTRO_NOMBRE: i32 = 13;

type ClaveRaya = (bool, bool, u16, i32, i32, i32, i32, String);
/// Lo que cambia dónde van las rayas que se ven: su forma, el desplazamiento y el alto de lo que se ve.
type VistaRayas = (ClaveRaya, i32, i32, i32, i32);

/// Lo que Bloc dibuja encima de lo que pinta RichEdit (ver `dibujar_rayas`), en píxeles de la ventana: el fondo de los
/// bloques de código y de las citas, las barras de las citas y las rayas.
struct Encima {
    codigo: Vec<RECT>,
    citas: Vec<RECT>,
    barras: Vec<RECT>,
    rayas: Vec<RECT>,
}
/// El diseño de cada tabla, por su texto.
type Disenos = HashMap<Vec<u16>, Option<Rc<tabla::Diseno>>>;
/// Lo que cambia el dibujo de las tablas que se ven: su diseño, el desplazamiento, el rectángulo del texto, la
/// selección, si el control tiene el foco y el modo noche.
type VistaTablas = (tabla::Clave, i32, i32, i32, i32, i32, (usize, usize), bool, bool);

/// Una tabla dibujada: dónde empieza, su esquina en la ventana, su diseño y si va entera seleccionada.
struct Dibujada {
    desde: usize,
    x: i32,
    y: i32,
    diseno: Rc<tabla::Diseno>,
    sel: bool,
}

pub struct App {
    pub hwnd: Cell<HWND>,
    pub ed: Cell<HWND>,
    pub botones: Cell<HWND>,
    pub estado: Cell<HWND>,
    pub buscador: Cell<HWND>,
    menu: Cell<HMENU>,
    menu_archivo: Cell<HMENU>,
    menu_recientes: Cell<HMENU>,
    menu_cod: Cell<HMENU>,
    menu_saltos: Cell<HMENU>,
    menu_margenes: Cell<HMENU>,
    pub barras: Barras,
    pub cfg: RefCell<Config>,
    ruta: RefCell<Option<PathBuf>>,
    cod: Cell<Cod>,
    saltos: Cell<Saltos>,
    cambiado: Cell<bool>,
    /// El original tenía caracteres nulos (se ven y se guardan como espacios).
    nulos: Cell<bool>,
    /// El original tenía bytes que no valen en su codificación: se ven como � y así se guardarían.
    invalidos: Cell<bool>,
    /// El control no ha guardado el texto tal cual (algún carácter que no admite).
    raro: Cell<bool>,
    /// Hora del archivo al abrirlo o guardarlo, para saber si alguien lo ha cambiado mientras.
    escrito: Cell<Option<SystemTime>>,
    /// La nota para la próxima vez, si la enseña esta ventana, y la vez que se escribió (ver `nota::Nota::serie`).
    nota: RefCell<Option<(nota::Tenencia, u64)>>,
    pub md: Cell<bool>,
    /// Markdown con formato en vivo: hay copia del texto y deshacer propio.
    pub vivo: Cell<bool>,
    aplicando: Cell<bool>,
    /// Dentro de una acción de Bloc (ver `accion`).
    en_accion: Cell<bool>,
    /// Dónde empieza la tabla en que estaba el cursor tras la última tecla, clic o acción (ver `mirar_tabla`).
    tabla_cursor: Cell<Option<usize>>,
    copia: RefCell<Vec<u16>>,
    nuevo: RefCell<Vec<u16>>,
    historial: RefCell<Historial>,
    /// Última línea calculada para la barra de estado: dónde empieza, su número y hasta dónde se sabe que no tiene
    /// saltos (en una línea larguísima, al teclear solo se lee lo nuevo).
    marca: Cell<(usize, usize, usize)>,
    hay_vallas: Cell<bool>,
    /// Repintando sin juntar las líneas (ver `sin_juntar`), con el control aún con sus md::JUNTO.
    desjuntando: Cell<bool>,
    /// Inicios de línea, de menor a mayor, en que se sabe qué bloque de código hay abierto (ver `abierto_en`).
    vallas_en: RefCell<Vec<(usize, Option<md::Valla>)>>,
    /// El centro y el alto de las rayas `---` (ver editor::rayas), con lo que los cambia: ajuste, ancho del texto, zoom
    /// y letra.
    forma_raya: Cell<Option<(ClaveRaya, editor::FormaRaya)>>,
    /// Dónde se dibujaron las rayas y los fondos la última vez, con la forma, el desplazamiento y el alto de lo que se
    /// veía.
    rayas_hechas: Cell<Option<(VistaRayas, Encima)>>,
    /// Lo dibujado encima antes del cambio que se está leyendo (ver `procesar`), por si al pintarlo sigue valiendo.
    encima_previa: Cell<Option<(VistaRayas, Encima)>>,
    /// Cuántas veces se ha vuelto a pintar desde el último cambio porque RichEdit no tenía colocadas las rayas.
    reintentos_raya: Cell<u8>,
    /// El diseño de cada tabla que se dibuja, por su texto, con el zoom, los puntos y el ancho de la clave.
    disenos: RefCell<(tabla::Clave, Disenos)>,
    /// Con qué zoom, puntos y ancho tienen su alto las filas plegadas del control (ver `dibujar_tablas`).
    clave_plegadas: Cell<Option<tabla::Clave>>,
    /// Dónde empiezan las tablas que se ven como texto (las demás se dibujan). Se abren las de los extremos de la
    /// selección (ver `mirar_tablas`).
    abiertas: RefCell<Vec<usize>>,
    /// Las tablas dibujadas la última vez, con lo que se veía.
    tablas_hechas: RefCell<Option<(VistaTablas, Vec<Dibujada>)>>,
    /// Se ve alguna tabla dibujada (ver `pintar`, que entonces deja repintar el control entero).
    tablas_vistas: Cell<bool>,
    /// Hay un WM_TABLAS en camino.
    tablas_avisadas: Cell<bool>,
    /// Inicio de la selección antes del último cambio (texto plano): si es antes de la marca, la marca ya no vale.
    sel_antes: Cell<usize>,
    menus_fallan: Cell<bool>,
    /// Sube con cada cambio del documento: al acabar un guardado de fondo dice si lo guardado sigue siendo lo último.
    rev: Cell<u64>,
    /// Ctrl+S escribe el archivo aparte para que la ventana no espere al disco.
    guardando: RefCell<Option<std::thread::JoinHandle<Guardado>>>,
    /// Se ha vuelto a pedir guardar mientras se guardaba: al acabar, otra vez.
    otra_vez: Cell<bool>,
    /// Número del último guardado de fondo: el aviso de uno anterior, ya atendido, no espera al que está en marcha.
    ronda: Cell<usize>,
    /// Lo que se acaba de poner de una vez (deshacer, rehacer, pegar), que se pinta entero: coge la letra del sitio en
    /// que cae, y la diferencia de textos empieza más abajo si repite lo que lo sigue o no cuenta si es igual.
    forzado: Cell<Option<(usize, usize)>>,
    /// La pareja vacía que acaba de poner un botón de formato (dónde quedó el cursor y qué efecto), hasta el siguiente
    /// cambio: pulsarlo otra vez la quita aunque, al principio de una línea, «~~~~» haya abierto un bloque de código.
    pareja: Cell<Option<(usize, u8)>>,
    /// La vista previa del Explorador (ver `vista`): sin barras ni menús, solo para leer.
    vista: Cell<bool>,
    /// El modo de noche guardado, mientras esta ventana lleva el contrario al de otra (M10): es el que se guarda al
    /// cerrar, salvo que se cambie a mano.
    noche_guardada: Cell<Option<bool>>,
    /// M6: las imágenes de los `![…](ruta)` del documento, por su ruta, y el zoom y el ancho con que se colocaron.
    imagenes: RefCell<HashMap<PathBuf, Dibujo>>,
    clave_imagenes: Cell<Option<tabla::Clave>>,
    /// Cuántas veces se han soltado: lo que cargue un hilo de una vez anterior no vale.
    vez_imagenes: Cell<u32>,
}

/// Una imagen de un .md: cargándose en otro hilo, lista (su mapa de bits, su tamaño y el de la última vez que se
/// escaló para dibujarla) o que no se puede ver.
enum Dibujo {
    Cargando,
    Lista { mapa: usize, ancho: i32, alto: i32, escalada: Option<(i32, usize)> },
    Falla,
}

/// Lo que deja la escritura del archivo para la ventana.
struct Guardado {
    ruta: PathBuf,
    /// Revisión y codificación del documento cuando se mandó guardar.
    rev: u64,
    cod: Cod,
    resultado: Result<Cod, fichero::ErrorGuardar>,
    hora: Option<SystemTime>,
}

const WM_GUARDADO: u32 = WM_APP + 1;
/// Mirar qué tablas se abren o se cierran, tras un cambio de la selección que no ha hecho una tecla ni un clic.
const WM_TABLAS: u32 = WM_APP + 2;
/// Volver a esconder las marcas de los párrafos de los que ha salido la selección (ver `tapar`).
const WM_TAPAR: u32 = WM_APP + 3;
/// Hay imágenes de un .md en `CARGADAS`, o hay otro zoom o ancho: se vuelven a colocar las líneas de las imágenes (M6).
const WM_IMAGEN: u32 = WM_APP + 7;
/// Una imagen cargada en otro hilo: en qué tanda (`vez_imagenes`), su ruta y, si se puede ver, su mapa de bits (como
/// número), ancho y alto.
type Cargada = (u32, PathBuf, Option<(usize, i32, i32)>);
/// Las que han cargado los hilos, para el siguiente WM_IMAGEN: por el mensaje solo va el aviso, no un puntero.
static CARGADAS: std::sync::Mutex<Vec<Cargada>> = std::sync::Mutex::new(Vec::new());
/// Lo que pueden ocupar juntas las imágenes cargadas de los .md; las que no quepan se quedan en texto.
const TOPE_IMAGENES: i64 = 256 << 20;
/// Cuántas se cargan a la vez como mucho: las demás, cuando acabe una.
const CARGANDO_A_LA_VEZ: usize = 4;

static APP: AtomicPtr<App> = AtomicPtr::new(std::ptr::null_mut());

pub fn app() -> &'static App {
    unsafe { &*APP.load(Ordering::Relaxed) }
}

fn px(n: i32) -> i32 {
    n * barras::dpi() / 96
}

/// El estado de la ventana, uno por proceso.
fn nueva_app() -> &'static App {
    let a = Box::leak(Box::new(App {
        hwnd: Cell::new(std::ptr::null_mut()),
        ed: Cell::new(std::ptr::null_mut()),
        botones: Cell::new(std::ptr::null_mut()),
        estado: Cell::new(std::ptr::null_mut()),
        buscador: Cell::new(std::ptr::null_mut()),
        menu: Cell::new(std::ptr::null_mut()),
        menu_archivo: Cell::new(std::ptr::null_mut()),
        menu_recientes: Cell::new(std::ptr::null_mut()),
        menu_cod: Cell::new(std::ptr::null_mut()),
        menu_saltos: Cell::new(std::ptr::null_mut()),
        menu_margenes: Cell::new(std::ptr::null_mut()),
        barras: Barras::nuevas(),
        cfg: RefCell::new(Config::cargar()),
        ruta: RefCell::new(None),
        cod: Cell::new(Cod::Utf8),
        saltos: Cell::new(Saltos::Crlf),
        cambiado: Cell::new(false),
        nulos: Cell::new(false),
        invalidos: Cell::new(false),
        raro: Cell::new(false),
        escrito: Cell::new(None),
        nota: RefCell::new(None),
        md: Cell::new(false),
        vivo: Cell::new(false),
        aplicando: Cell::new(false),
        en_accion: Cell::new(false),
        tabla_cursor: Cell::new(None),
        copia: RefCell::new(Vec::new()),
        nuevo: RefCell::new(Vec::new()),
        historial: RefCell::new(Historial::default()),
        marca: Cell::new((0, 1, 0)),
        hay_vallas: Cell::new(false),
        desjuntando: Cell::new(false),
        vallas_en: RefCell::new(Vec::new()),
        forma_raya: Cell::new(None),
        rayas_hechas: Cell::new(None),
        encima_previa: Cell::new(None),
        reintentos_raya: Cell::new(0),
        disenos: RefCell::new(((0, 0, 0), HashMap::new())),
        clave_plegadas: Cell::new(None),
        abiertas: RefCell::new(Vec::new()),
        tablas_hechas: RefCell::new(None),
        tablas_vistas: Cell::new(false),
        tablas_avisadas: Cell::new(false),
        sel_antes: Cell::new(0),
        menus_fallan: Cell::new(false),
        rev: Cell::new(0),
        guardando: RefCell::new(None),
        otra_vez: Cell::new(false),
        ronda: Cell::new(0),
        forzado: Cell::new(None),
        pareja: Cell::new(None),
        vista: Cell::new(false),
        noche_guardada: Cell::new(None),
        imagenes: RefCell::new(HashMap::new()),
        clave_imagenes: Cell::new(None),
        vez_imagenes: Cell::new(0),
    }));
    APP.store(a, Ordering::SeqCst);
    a
}

/// En cada ventana de Bloc, su modo (1 claro, 2 oscuro), para que la siguiente sepa cuál lleva la de delante (M10).
const PROP_NOCHE: &[u16] = &[66, 108, 111, 99, 78, 111, 99, 104, 101, 0]; // «BlocNoche»

/// Si la última ventana de Bloc que estaba delante va en oscuro (la primera visible por orden de delante a atrás), o
/// nada si no hay otra. Es buscar ventanas y leer una propiedad: microsegundos.
fn noche_de_otra() -> Option<bool> {
    let clase = ws("BlocVentana");
    let mut h = std::ptr::null_mut();
    loop {
        h = unsafe { FindWindowExW(std::ptr::null_mut(), h, clase.as_ptr(), std::ptr::null()) };
        if h.is_null() {
            return None;
        }
        let modo = unsafe { GetPropW(h, PROP_NOCHE.as_ptr()) } as usize;
        if unsafe { IsWindowVisible(h) } != 0 && modo != 0 {
            return Some(modo == 2);
        }
    }
}

/// La clase de la ventana de Bloc, que usan también la vista previa.
fn registrar_clase() -> Vec<u16> {
    let clase = ws("BlocVentana");
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let c = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(proc_ventana),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: LoadIconW(inst, 1 as _),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // Sin fondo: lo cubren las barras y el texto, y borrarlo en blanco antes de pintarlos destellaba al agrandar.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: clase.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&c);
    }
    clase
}

/// La vista previa del Explorador (ver previa.rs): una ventana de Bloc hija de `padre`, sin barras ni menús, que solo
/// enseña. Con las marcas ocultas, ajuste de línea, al 100 % y en claro u oscuro como Windows; la configuración de
/// Bloc no se toca. Una por proceso.
pub fn vista(padre: HWND, r: &RECT) -> HWND {
    let a = nueva_app();
    a.vista.set(true);
    {
        let mut cfg = a.cfg.borrow_mut();
        (cfg.marcas, cfg.ajuste, cfg.barra, cfg.zoom, cfg.corrector, cfg.copia) = (false, true, false, 100, false, false);
        cfg.noche = noche_vista();
    }
    let clase = registrar_clase();
    unsafe {
        let m = ws("msftedit.dll");
        LoadLibraryExW(m.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        CreateWindowExW(
            0,
            clase.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            padre,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    }
}

/// La vista previa sigue a Windows en claro u oscuro, también si ha cambiado desde la vez anterior.
/// Como Windows; las pruebas lo eligen con BLOC_NOCHE (el Explorador no deja cambiarlo sin cambiar el de Windows).
fn noche_vista() -> bool {
    std::env::var("BLOC_NOCHE").map_or_else(|_| tema::como_windows(), |v| v == "1")
}

pub fn vista_tema() {
    let a = app();
    let noche = noche_vista();
    if a.cfg.borrow().noche != noche {
        a.cfg.borrow_mut().noche = noche;
        colores();
    }
}

/// Lo que enseña la vista previa: un archivo ya leído, como Markdown o como texto.
pub fn vista_cargar(l: fichero::Leido, md: bool) {
    cargar_como(l.texto, None, l.cod, l.saltos, l.nulos, md);
}

/// Lo que falta de un texto largo de la vista previa, al final y sin deshacer.
pub fn vista_anadir(t: &[u16]) {
    let ed = app().ed.get();
    editor::deshacer_nativo(ed, false);
    editor::anadir(ed, t);
}

pub fn ejecutar(archivo: Option<OsString>, instalado: bool) -> i32 {
    unsafe {
        OleInitialize(std::ptr::null());
        // Solo la de Windows: por su nombre, se buscaría antes junto al .exe (una copia puesta ahí en la carpeta de red).
        let m = ws("msftedit.dll");
        LoadLibraryExW(m.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        // Si Windows reinicia por una actualización con algo sin guardar, vuelve a abrir Bloc con la nota (ver main).
        RegisterApplicationRestart(ws("/reinicio").as_ptr(), RESTART_NO_CRASH | RESTART_NO_HANG);
    }
    let a = nueva_app();
    // M10: con la opción, del color contrario al de la última ventana de Bloc que estaba delante; sin otra, el guardado.
    if a.cfg.borrow().contrario
        && let Some(otra) = noche_de_otra()
    {
        let guardada = a.cfg.borrow().noche;
        a.noche_guardada.set(Some(guardada));
        a.cfg.borrow_mut().noche = !otra;
    }
    if a.cfg.borrow().noche {
        tema::menus(true);
    }
    barras::registrar();
    let clase = registrar_clase();
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let mut zona = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut zona as *mut _ as *mut _, 0);
        let titulo = ws("Bloc");
        let h = CreateWindowExW(
            0,
            clase.as_ptr(),
            titulo.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (zona.right - zona.left) * 55 / 100,
            (zona.bottom - zona.top) * 70 / 100,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null(),
        );
        // La posición de la última vez, si sigue cayendo en alguna pantalla.
        if let Some((x, y, w, alto)) = a.cfg.borrow().marco {
            let r = RECT { left: x, top: y, right: x + w, bottom: y + alto };
            if w > 200 && alto > 150 && !MonitorFromRect(&r, MONITOR_DEFAULTTONULL).is_null() {
                let mut p: WINDOWPLACEMENT = std::mem::zeroed();
                p.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
                GetWindowPlacement(h, &mut p);
                p.rcNormalPosition = r;
                p.showCmd = SW_HIDE as u32;
                SetWindowPlacement(h, &p);
            }
        }
        let recuperada = crate::copia::recuperar();
        abrir_inicial(archivo);
        if recuperada {
            poner_mensaje(t().recuperado);
        }
        // Ya con la ruta del archivo completa: abierto con doble clic, la carpeta de trabajo es la suya, y Windows no
        // dejaría cambiarle el nombre ni moverla mientras Bloc siga abierto.
        let _ = std::env::set_current_dir(std::env::temp_dir());
        if instalado {
            poner_mensaje(t().instalado);
            // La lista de la barra de tareas se pone con la ventana ya a la vista: Windows la guarda.
            SetTimer(h, RELOJ_LISTA, 400, None);
        }
        ShowWindow(h, if a.cfg.borrow().max { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL });
        // El corrector tarda en ponerse en marcha (~75 ms): primero se enseña el texto.
        UpdateWindow(h);
        crate::instalar::buscar_actualizacion(h);
        crate::informe::al_abrir();
        if a.cfg.borrow().corrector {
            editor::corrector(a.ed.get(), true);
        }
        let atajos = crear_atajos();
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 {
            let b = a.buscador.get();
            let en_panel = !b.is_null() && IsChild(b, m.hwnd) != 0;
            if en_panel && seguro(|| buscar::tecla(&m) as LRESULT, || 0) != 0 {
                continue;
            }
            // En las casillas del panel, Ctrl+C copia lo que hay en ella y no lo del texto.
            if !(en_panel && buscar::de_casilla(&m)) && (m.hwnd == h || IsChild(h, m.hwnd) != 0) && TranslateAcceleratorW(h, atajos, &m) != 0 {
                continue;
            }
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        m.wParam as i32
    }
}

fn crear_atajos() -> HACCEL {
    let ctrl = FVIRTKEY | FCONTROL;
    let ctrl_mayus = FVIRTKEY | FCONTROL | FSHIFT;
    let solo = FVIRTKEY;
    let l = |ch: u8| ch as u16;
    let v: Vec<ACCEL> = [
        (ctrl, l(b'N'), c::NUEVO),
        (ctrl_mayus, l(b'N'), c::VENTANA),
        (ctrl, l(b'O'), c::ABRIR),
        (ctrl, l(b'S'), c::GUARDAR),
        (ctrl_mayus, l(b'S'), c::GUARDAR_COMO),
        (ctrl, l(b'P'), c::IMPRIMIR),
        (ctrl, l(b'Z'), c::DESHACER),
        (ctrl, l(b'Y'), c::REHACER),
        (ctrl_mayus, l(b'Z'), c::REHACER),
        (ctrl, l(b'X'), c::CORTAR),
        (ctrl, l(b'C'), c::COPIAR),
        (ctrl, l(b'V'), c::PEGAR),
        (ctrl_mayus, l(b'V'), c::PEGAR_TEXTO),
        (ctrl, l(b'A'), c::TODO),
        (ctrl, l(b'F'), c::BUSCAR),
        (solo, VK_F3, c::SIGUIENTE),
        (FVIRTKEY | FSHIFT, VK_F3, c::ANTERIOR),
        (ctrl, l(b'H'), c::REEMPLAZAR),
        (ctrl, l(b'G'), c::IR),
        (solo, VK_F5, c::HORA),
        (ctrl, l(b'B'), c::NEGRITA),
        (ctrl, l(b'I'), c::CURSIVA),
        (ctrl, l(b'K'), c::ENLACE),
        (ctrl, l(b'1'), c::TITULO1),
        (ctrl, l(b'2'), c::TITULO2),
        (ctrl, l(b'3'), c::TITULO3),
        (ctrl_mayus, l(b'T'), c::ALINEAR),
        (FVIRTKEY | FALT, l(b'Z'), c::AJUSTE),
        (ctrl_mayus, l(b'D'), c::NOCHE),
        (ctrl_mayus, l(b'M'), c::MARCAS),
        (ctrl, VK_OEM_PLUS, c::ACERCAR),
        (ctrl, VK_ADD, c::ACERCAR),
        (ctrl, VK_OEM_MINUS, c::ALEJAR),
        (ctrl, VK_SUBTRACT, c::ALEJAR),
        (ctrl, l(b'0'), c::NORMAL),
        (ctrl, VK_NUMPAD0, c::NORMAL),
    ]
    .iter()
    .map(|&(f, k, id)| ACCEL { fVirt: f, key: k, cmd: id })
    .collect();
    unsafe { CreateAcceleratorTableW(v.as_ptr(), v.len() as i32) }
}

unsafe extern "system" fn proc_ventana(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Si algo falla al cerrar, mejor no cerrar: el texto sigue ahí para guardarlo.
    seguro(|| mensaje_ventana(h, m, wp, lp), || if m == WM_CLOSE { 0 } else { unsafe { DefWindowProcW(h, m, wp, lp) } })
}

fn mensaje_ventana(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let a = app();
    match m {
        WM_CREATE => {
            a.hwnd.set(h);
            if a.vista.get() {
                a.ed.set(editor::crear(h, ID_EDITOR));
                // Solo para leer: ni se escribe ni se suelta nada (abriría lo soltado dentro del Explorador).
                unsafe { windows_sys::Win32::UI::Shell::DragAcceptFiles(a.ed.get(), 0) };
                enviar(a.ed.get(), EM_SETREADONLY, 1, 0);
                editor::ajuste(a.ed.get(), true);
                colores();
            } else {
                crear_hijos(h);
            }
            0
        }
        WM_SIZE => {
            if wp != SIZE_MINIMIZED as usize {
                colocar();
            }
            0
        }
        // Sin barra de título, la franja de arriba es la que se arrastra para mover la ventana (y se pinta como la barra).
        WM_ERASEBKGND => {
            let t = tira();
            if t > 0 {
                let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                unsafe {
                    GetClientRect(h, &mut rc);
                    let pincel = CreateSolidBrush(tema::paleta(a.cfg.borrow().noche).barra);
                    FillRect(wp as HDC, &RECT { bottom: t, ..rc }, pincel);
                    DeleteObject(pincel as _);
                }
            }
            1
        }
        WM_NCHITTEST => {
            let r = unsafe { DefWindowProcW(h, m, wp, lp) };
            let mut p = POINT { x: (lp & 0xFFFF) as i16 as i32, y: ((lp >> 16) & 0xFFFF) as i16 as i32 };
            unsafe { ScreenToClient(h, &mut p) };
            if r == HTCLIENT as LRESULT && p.y < tira() { HTCAPTION as LRESULT } else { r }
        }
        WM_SETFOCUS => {
            editor::rastro("ventana:foco");
            unsafe { SetFocus(a.ed.get()) };
            0
        }
        WM_ACTIVATE => {
            editor::rastro(match (wp & 0xFFFF) as u32 {
                WA_INACTIVE => "ventana:inactiva",
                WA_CLICKACTIVE => "ventana:activa por clic",
                _ => "ventana:activa",
            });
            // Una imagen que no estaba puede estar ahora (se ha copiado al volver del Explorador): se vuelve a probar.
            if (wp & 0xFFFF) as u32 != WA_INACTIVE {
                let fallidas: std::collections::HashSet<PathBuf> = {
                    let mut imagenes = a.imagenes.borrow_mut();
                    let v = imagenes.iter().filter(|x| matches!(x.1, Dibujo::Falla)).map(|x| x.0.clone()).collect();
                    imagenes.retain(|_, d| !matches!(d, Dibujo::Falla));
                    v
                };
                if !fallidas.is_empty() {
                    colocar_imagenes(Some(&fallidas));
                }
            }
            unsafe { DefWindowProcW(h, m, wp, lp) }
        }
        WM_COMMAND => {
            if lp as HWND == a.ed.get() {
                if (wp >> 16) as u32 == EN_CHANGE {
                    editor::al_cambiar();
                }
            } else {
                accion(|| comando((wp & 0xFFFF) as u16));
            }
            0
        }
        WM_NOTIFY => {
            let n = unsafe { &*(lp as *const NMHDR) };
            if n.hwndFrom == a.ed.get() && n.code == re::EN_SELCHANGE {
                al_mover();
            }
            0
        }
        WM_INITMENUPOPUP => {
            if wp as HMENU == a.menu_archivo.get() {
                llenar_recientes();
            }
            estados_menu(wp as HMENU);
            0
        }
        crate::instalar::WM_ACTUALIZADO => {
            poner_mensaje(&crate::instalar::aviso_actualizacion(wp));
            0
        }
        crate::informe::WM_INFORMADO => {
            poner_mensaje(crate::informe::aviso(wp));
            0
        }
        crate::instalar::WM_ELEGIR => {
            elegir(true);
            0
        }
        WM_TABLAS => {
            mirar_tablas();
            0
        }
        WM_IMAGEN => {
            let hechas = std::mem::take(&mut *CARGADAS.lock().unwrap_or_else(|e| e.into_inner()));
            let mut nuevas = std::collections::HashSet::new();
            let mut imagenes = a.imagenes.borrow_mut();
            for (vez, ruta, cargada) in hechas {
                // De antes de soltarlas (otro documento, otro fondo): ya no valen.
                if vez != a.vez_imagenes.get() {
                    if let Some((mapa, ..)) = cargada {
                        unsafe { DeleteObject(mapa as _) };
                    }
                    continue;
                }
                let usado: i64 = imagenes.values().map(|d| if let Dibujo::Lista { ancho, alto, .. } = d { *ancho as i64 * *alto as i64 * 4 } else { 0 }).sum();
                let d = match cargada {
                    Some((mapa, ancho, alto)) if usado + ancho as i64 * alto as i64 * 4 <= TOPE_IMAGENES => Dibujo::Lista { mapa, ancho, alto, escalada: None },
                    Some((mapa, ..)) => {
                        unsafe { DeleteObject(mapa as _) };
                        Dibujo::Falla
                    }
                    None => Dibujo::Falla,
                };
                imagenes.insert(ruta.clone(), d);
                nuevas.insert(ruta);
            }
            drop(imagenes);
            colocar_imagenes(if lp == 0 { None } else { Some(&nuevas) });
            0
        }
        WM_TAPAR => {
            tapar(false);
            0
        }
        WM_GUARDADO => {
            if wp == a.ronda.get() && esperar_guardado() {
                guardar(false, true);
            }
            0
        }
        WM_TIMER if wp == RELOJ_VISTA => {
            tramo_vista();
            0
        }
        WM_TIMER if wp == RELOJ_TABLAS => {
            unsafe { KillTimer(h, RELOJ_TABLAS) };
            mirar_tablas();
            0
        }
        WM_TIMER if wp == crate::copia::RELOJ => {
            crate::copia::reloj(h, || {
                (a.cambiado.get() && a.cfg.borrow().copia).then(|| {
                    let mut n = nota_de_ahora();
                    // De qué nota sale: al recuperarla, la sustituye en vez de repetirla al final.
                    n.serie = a.nota.borrow().as_ref().map_or(0, |(_, s)| *s);
                    n
                })
            });
            0
        }
        crate::imprimir::WM_PAGINAR => {
            crate::imprimir::paginar_aqui(lp);
            0
        }
        crate::imprimir::WM_NO_IMPRESO => {
            aviso(t().no_impreso);
            0
        }
        WM_TIMER if wp == crate::imprimir::RELOJ => {
            crate::imprimir::reintentar();
            0
        }
        WM_TIMER if wp == RELOJ_LISTA => {
            unsafe { KillTimer(h, RELOJ_LISTA) };
            crate::instalar::lista_de_tareas();
            0
        }
        // Al apagar o cerrar la sesión, lo mismo que al cerrar: si hay cambios, se pregunta (y se puede parar el apagado).
        // Al apagar o cerrar la sesión, sin preguntar: lo que haya sin guardar va a la nota para la próxima vez. Solo si no
        // se puede escribir, se pregunta como al cerrar.
        WM_QUERYENDSESSION => (!a.cambiado.get() || seguir(false) || puedo_descartar()) as LRESULT,
        WM_ENDSESSION => {
            // La vista previa no guarda nada: su configuración es la suya (sin barra, al 100 %...), no la de Bloc.
            if wp != 0 && !a.vista.get() {
                guardar_config();
            }
            0
        }
        WM_CLOSE => {
            if puedo_cerrar() {
                guardar_config();
                unsafe { DestroyWindow(h) };
            }
            0
        }
        WM_DESTROY => {
            unsafe { RemovePropW(h, PROP_NOCHE.as_ptr()) };
            // Solo se llega aquí con lo de esta ventana guardado, descartado o en la nota.
            crate::copia::borrar(h);
            unsafe { PostQuitMessage(0) };
            0
        }
        0x91 | 0x92 | WM_NCPAINT | WM_NCACTIVATE if a.cfg.borrow().noche && !a.menus_fallan.get() => menus_oscuros(h, m, wp, lp),
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

fn crear_hijos(h: HWND) {
    let a = app();
    barras::crear_fuentes();
    a.ed.set(editor::crear(h, ID_EDITOR));
    a.botones.set(barras::crear(h, "BlocBotones", ID_BOTONES));
    a.estado.set(barras::crear(h, "BlocEstado", ID_ESTADO));
    barras::poner_botones(a.botones.get(), botones());
    crear_menu(h);
    let ed = a.ed.get();
    let cfg = a.cfg.borrow();
    editor::ajuste(ed, cfg.ajuste);
    editor::zoom(ed, cfg.zoom);
    if cfg.barra {
        unsafe { ShowWindow(a.estado.get(), SW_SHOWNA) };
    }
    drop(cfg);
    colores();
}

// Botón con su ayuda: la frase del menú y su atajo.
fn boton(id: u16, icono: Icono, menu: &str, grupo: bool) -> Boton {
    Boton::nuevo(id, icono, crate::textos::con_atajo(menu), grupo)
}

fn botones() -> Vec<Boton> {
    use Icono::{Cita, Numerada, Simbolo, Titulo};
    let t = t();
    vec![
        boton(c::NEGRITA, Simbolo(0xE8DD), t.negrita, false),
        boton(c::CURSIVA, Simbolo(0xE8DB), t.cursiva, false),
        boton(c::TACHADO, Simbolo(0xEDE0), t.tachado, false),
        boton(c::CODIGO, Simbolo(0xE943), t.codigo, false),
        boton(c::ENLACE, Simbolo(0xE71B), t.enlace, false),
        boton(c::TITULO1, Titulo(1), t.titulo1, true),
        boton(c::TITULO2, Titulo(2), t.titulo2, false),
        boton(c::TITULO3, Titulo(3), t.titulo3, false),
        boton(c::LISTA, Simbolo(0xE8FD), t.lista, true),
        boton(c::NUMERADA, Numerada, t.numerada, false),
        boton(c::CASILLAS, Simbolo(0xE9D5), t.casillas, false),
        boton(c::CITA, Cita, t.cita, false),
        boton(c::TABLA, Simbolo(0xE80A), t.tabla, true),
        boton(c::ALINEAR, Simbolo(0xE8E4), t.alinear, false),
        boton(c::BUSCAR, Simbolo(0xE721), t.buscar, true).a_la_derecha(),
    ]
}

fn anadir(m: HMENU, id: u16, texto: &str) {
    let t = ws(texto);
    unsafe { AppendMenuW(m, MF_STRING, id as usize, t.as_ptr()) };
}

fn raya(m: HMENU) {
    unsafe { AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null()) };
}

fn submenu(m: HMENU, sub: HMENU, texto: &str) {
    let t = ws(texto);
    unsafe { AppendMenuW(m, MF_POPUP, sub as usize, t.as_ptr()) };
}

fn crear_menu(h: HWND) {
    let a = app();
    let t = t();
    unsafe {
        let barra = CreateMenu();
        let archivo = CreatePopupMenu();
        let cod = CreatePopupMenu();
        let saltos = CreatePopupMenu();
        anadir(archivo, c::NUEVO, t.nuevo);
        anadir(archivo, c::VENTANA, t.nueva_ventana);
        anadir(archivo, c::ABRIR, t.abrir);
        let recientes = CreatePopupMenu();
        submenu(archivo, recientes, t.recientes);
        anadir(archivo, c::GUARDAR, t.guardar);
        anadir(archivo, c::GUARDAR_COMO, t.guardar_como);
        anadir(archivo, c::COPIA, t.copia);
        raya(archivo);
        for (i, n) in ["UTF-8", t.utf8_bom, "UTF-16 LE", "UTF-16 BE", "ANSI"].iter().enumerate() {
            anadir(cod, c::COD + i as u16, n);
        }
        for (i, n) in [t.crlf, t.lf, t.cr].iter().enumerate() {
            anadir(saltos, c::SALTOS + i as u16, n);
        }
        submenu(archivo, cod, t.codificacion);
        submenu(archivo, saltos, t.saltos);
        raya(archivo);
        anadir(archivo, c::IMPRIMIR, t.imprimir);
        raya(archivo);
        // Solo en el instalado: lo elegido se pone para él.
        if crate::instalar::es_instalado() {
            anadir(archivo, c::ELEGIR, t.m_eleccion);
        }
        anadir(archivo, c::INFORMAR, t.informar_menu);
        anadir(archivo, c::SALIR, t.salir);

        let edicion = CreatePopupMenu();
        for (id, s) in [(c::DESHACER, t.deshacer), (c::REHACER, t.rehacer)] {
            anadir(edicion, id, s);
        }
        raya(edicion);
        for (id, s) in [(c::CORTAR, t.cortar), (c::COPIAR, t.copiar), (c::PEGAR, t.pegar), (c::PEGAR_TEXTO, t.pegar_texto), (c::ELIMINAR, t.eliminar), (c::TODO, t.seleccionar_todo)] {
            anadir(edicion, id, s);
        }
        raya(edicion);
        for (id, s) in [(c::BUSCAR, t.buscar), (c::SIGUIENTE, t.buscar_siguiente), (c::ANTERIOR, t.buscar_anterior), (c::REEMPLAZAR, t.reemplazar), (c::IR, t.ir_a_linea)] {
            anadir(edicion, id, s);
        }
        raya(edicion);
        anadir(edicion, c::HORA, t.hora_fecha);

        let ver = CreatePopupMenu();
        for (id, s) in [(c::AJUSTE, t.ajuste), (c::BARRA, t.barra_estado), (c::NOCHE, t.noche), (c::CORRECTOR, t.corrector), (c::MARCAS, t.marcas), (c::NUEVOS_MD, t.nuevos_md), (c::CONTRARIO, t.contrario)] {
            anadir(ver, id, s);
        }
        raya(ver);
        anadir(ver, c::LETRA, t.letra);
        let margenes = CreatePopupMenu();
        for (i, n) in t.anchos_margen.iter().enumerate() {
            anadir(margenes, c::MARGENES + i as u16, n);
        }
        submenu(ver, margenes, t.margenes);
        a.menu_margenes.set(margenes);
        raya(ver);
        for (id, s) in [(c::ACERCAR, t.acercar), (c::ALEJAR, t.alejar), (c::NORMAL, t.tamano_normal)] {
            anadir(ver, id, s);
        }

        submenu(barra, archivo, t.m_archivo);
        submenu(barra, edicion, t.m_edicion);
        submenu(barra, ver, t.m_ver);
        SetMenu(h, barra);
        a.menu.set(barra);
        a.menu_cod.set(cod);
        a.menu_saltos.set(saltos);
        a.menu_archivo.set(archivo);
        a.menu_recientes.set(recientes);
    }
}

/// Archivo › Recientes, con la lista de ahora: se llena al abrir Archivo, así sale con lo que hayan abierto las demás
/// ventanas. Si no hay ninguno, en gris.
fn llenar_recientes() {
    let a = app();
    let (archivo, m) = (a.menu_archivo.get(), a.menu_recientes.get());
    let lista = crate::config::recientes();
    unsafe {
        while GetMenuItemCount(m) > 0 {
            DeleteMenu(m, 0, MF_BYPOSITION);
        }
    }
    let hay_nota = a.nota.borrow().is_none() && nota::libre();
    if hay_nota {
        anadir(m, c::NOTA, t().nota_ultima);
        raya(m);
    }
    for (i, r) in lista.iter().enumerate() {
        let nombre = r.file_name().unwrap_or(r.as_os_str()).to_string_lossy().replace('&', "&&");
        let carpeta = r.parent().map(|p| crate::textos::acortar(&p.to_string_lossy())).unwrap_or_default().replace('&', "&&");
        anadir(m, c::RECIENTE + i as u16, &format!("&{} {nombre}\t{carpeta}", i + 1));
    }
    if !lista.is_empty() {
        raya(m);
    }
    anadir(m, c::BORRAR_RECIENTES, t().borrar_lista);
    unsafe {
        if let Some(i) = (0..GetMenuItemCount(archivo)).find(|&i| GetSubMenu(archivo, i) == m) {
            EnableMenuItem(archivo, i as u32, MF_BYPOSITION | if lista.is_empty() && !hay_nota { MF_GRAYED } else { MF_ENABLED });
        }
    }
}

/// Solo al pulsarlo se mira si sigue ahí. Si tampoco se ve su carpeta (una unidad de red apagada), se intenta abrir y
/// se dice por qué no se puede, sin quitarlo: puede volver.
fn abrir_reciente(i: usize) {
    let Some(r) = crate::config::recientes().into_iter().nth(i) else { return };
    if !r.exists() && r.parent().is_some_and(Path::exists) {
        crate::config::quitar_reciente(&r);
        aviso(&con(t().no_esta, r.display()));
        return;
    }
    if puedo_descartar() {
        abrir(&r);
    }
}

/// Sin barra de título (ventana muy baja), la franja de arriba del texto por la que se arrastra la ventana.
fn tira() -> i32 {
    let h = app().hwnd.get();
    if unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32 & WS_CAPTION == WS_CAPTION { 0 } else { px(6) }
}

/// La barra de título con el marco sale si pasa del 25 % del alto de la ventana, y vuelve al agrandarla (M3). Dice si
/// la ha quitado o puesto: entonces Windows vuelve a mandar WM_SIZE y se coloca todo con ese.
fn titulo_segun_alto(alto: i32) -> bool {
    let h = app().hwnd.get();
    // Con el manifiesto (dpiAware), las medidas de Windows ya van a los puntos por pulgada de la pantalla.
    let barra = unsafe { GetSystemMetrics(SM_CYCAPTION) + GetSystemMetrics(SM_CYSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER) };
    let quitar = unsafe { IsZoomed(h) } == 0 && barra * 4 > alto;
    let estilo = unsafe { GetWindowLongPtrW(h, GWL_STYLE) };
    let tiene = estilo as u32 & WS_CAPTION == WS_CAPTION;
    if tiene != quitar {
        return false;
    }
    let nuevo = if quitar { estilo & !(WS_CAPTION as isize) } else { estilo | WS_CAPTION as isize };
    unsafe {
        SetWindowLongPtrW(h, GWL_STYLE, nuevo);
        SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    true
}

/// La barra de botones arriba, debajo el panel de buscar (con el alto que tenga mientras se despliega), el texto y la
/// barra de estado. Ninguna franja pasa del 25 % del alto de la ventana (M3): la que no cabe se esconde mientras tanto
/// (el panel de buscar, primero a solo su fila de buscar) y sus teclas siguen yendo; lo elegido en «Ver» no cambia.
pub fn colocar() {
    let a = app();
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(a.hwnd.get(), &mut rc) };
    let (w, h) = (rc.right, rc.bottom);
    if a.vista.get() {
        unsafe { MoveWindow(a.ed.get(), 0, 0, w, h, 1) };
        margenes();
        return;
    }
    let mut rv = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(a.hwnd.get(), &mut rv) };
    let alto = rv.bottom - rv.top;
    if titulo_segun_alto(alto) {
        return;
    }
    let cabe = |x: i32| x * 4 <= alto;
    let tira = tira();
    let arriba = if a.md.get() { barras::alto_botones(w) } else { 0 };
    let mut arriba = if cabe(arriba) { arriba } else { 0 };
    let panel = buscar::alto();
    let mut panel = if cabe(panel) { panel } else if cabe(buscar::fila()) { panel.min(buscar::fila()) } else { 0 };
    let abajo = if a.cfg.borrow().barra { barras::alto_estado() } else { 0 };
    let mut abajo = if cabe(abajo) { abajo } else { 0 };
    // Cada franja cabe en su 25 %, pero juntas pueden no dejar sitio al texto: al menos una línea, escondiendo en el
    // mismo orden (la fila de reemplazar, la barra de botones, el panel y la barra de estado).
    let poco = |arriba: i32, panel: i32, abajo: i32| h - tira - arriba - panel - abajo < px(24);
    if poco(arriba, panel, abajo) {
        panel = panel.min(buscar::fila());
    }
    if poco(arriba, panel, abajo) {
        arriba = 0;
    }
    if poco(arriba, panel, abajo) {
        panel = 0;
    }
    if poco(arriba, panel, abajo) {
        abajo = 0;
    }
    let texto = tira + arriba + panel;
    // Todos de una vez: se repintan juntos y no uno tras otro.
    unsafe {
        let mut d = BeginDeferWindowPos(4);
        let mut sitios = vec![(a.botones.get(), tira, arriba), (a.estado.get(), h - abajo, abajo), (a.ed.get(), texto, (h - texto - abajo).max(0))];
        if !a.buscador.get().is_null() {
            sitios.push((a.buscador.get(), tira + arriba, panel));
        }
        for (v, y, alto) in sitios {
            if !d.is_null() {
                d = DeferWindowPos(d, v, std::ptr::null_mut(), 0, y, w, alto, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        if !d.is_null() {
            EndDeferWindowPos(d);
        }
        // Al estrecharse, RichEdit solo repinta lo de dentro del texto: en el margen derecho se quedaba lo que Bloc había
        // dibujado encima (fondos y rayas) con el ancho de antes.
        InvalidateRect(a.ed.get(), std::ptr::null(), 0);
    }
    // Con el ancho nuevo (los del 5 % cambian con él; los demás, por si se pusieron antes de que la ventana tuviera su
    // tamaño).
    margenes();
}

fn comando(id: u16) {
    let a = app();
    let ed = a.ed.get();
    let md = a.md.get();
    match id {
        c::NUEVO => nuevo(),
        c::VENTANA => crate::instalar::otra_ventana(),
        c::ABRIR => abrir_dialogo(),
        c::RECIENTE..c::BORRAR_RECIENTES => abrir_reciente((id - c::RECIENTE) as usize),
        c::BORRAR_RECIENTES => crate::config::borrar_recientes(),
        c::COPIA => {
            let si = !a.cfg.borrow().copia;
            a.cfg.borrow_mut().copia = si;
            if si && a.cambiado.get() {
                crate::copia::cambio(a.hwnd.get());
            } else if !si {
                crate::copia::borrar(a.hwnd.get());
            }
        }
        c::NOTA => {
            if puedo_descartar() {
                abrir_nota();
            }
        }
        c::GUARDAR => {
            guardar(false, true);
        }
        c::GUARDAR_COMO => {
            guardar(true, false);
        }
        c::IMPRIMIR => crate::imprimir::imprimir(),
        c::ELEGIR => elegir(false),
        c::INFORMAR => crate::informe::preguntar(),
        c::SALIR => unsafe {
            PostMessageW(a.hwnd.get(), WM_CLOSE, 0, 0);
        },
        c::DESHACER => deshacer(),
        c::REHACER => rehacer(),
        c::CORTAR => {
            enviar(ed, WM_CUT, 0, 0);
        }
        c::COPIAR => {
            enviar(ed, WM_COPY, 0, 0);
        }
        c::PEGAR => {
            enviar(ed, WM_PASTE, 0, 0);
        }
        c::PEGAR_TEXTO => editor::pegar(ed, false),
        c::ELIMINAR => {
            enviar(ed, WM_CLEAR, 0, 0);
        }
        c::TODO => {
            editor::seleccionar(ed, 0, editor::largo(ed));
            estado();
        }
        c::BUSCAR => buscar::mostrar(false),
        c::REEMPLAZAR => buscar::mostrar(true),
        c::SIGUIENTE => buscar::buscar(true),
        c::ANTERIOR => buscar::buscar(false),
        c::IR => buscar::ir_a_linea(),
        c::HORA => editor::sustituir(ed, &fecha()),
        c::NEGRITA if md => envolver(md::NEGRITA, "**"),
        c::CURSIVA if md => envolver(md::CURSIVA, "*"),
        c::TACHADO if md => envolver(md::TACHADO, "~~"),
        c::CODIGO if md => envolver(md::CODIGO, "`"),
        c::ENLACE if md => enlace(),
        c::TITULO1 if md => encabezar(1),
        c::TITULO2 if md => encabezar(2),
        c::TITULO3 if md => encabezar(3),
        c::LISTA if md => listar(0),
        c::NUMERADA if md => listar(1),
        c::CASILLAS if md => listar(2),
        c::CITA if md => listar(3),
        c::TABLA if md => insertar_tabla(),
        c::ALINEAR if md => alinear_tablas(),
        c::AJUSTE => {
            let si = !a.cfg.borrow().ajuste;
            a.cfg.borrow_mut().ajuste = si;
            editor::ajuste(ed, si);
            // Sin ajuste de línea no se juntan las líneas partidas (ver `pintar`).
            if a.vivo.get() {
                pintar_todo();
            }
            editor::ver_cursor(ed);
        }
        c::BARRA => {
            let si = !a.cfg.borrow().barra;
            a.cfg.borrow_mut().barra = si;
            unsafe { ShowWindow(a.estado.get(), if si { SW_SHOWNA } else { SW_HIDE }) };
            colocar();
            estado();
        }
        c::CONTRARIO => {
            let si = !a.cfg.borrow().contrario;
            a.cfg.borrow_mut().contrario = si;
        }
        c::NOCHE => {
            a.noche_guardada.set(None);
            let si = !a.cfg.borrow().noche;
            a.cfg.borrow_mut().noche = si;
            tema::menus(si);
            colores();
            buscar::pintar();
            unsafe { RedrawWindow(a.hwnd.get(), std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW) };
        }
        c::CORRECTOR => {
            let si = !a.cfg.borrow().corrector;
            a.cfg.borrow_mut().corrector = si;
            editor::corrector(ed, si);
        }
        c::MARCAS => {
            let si = !a.cfg.borrow().marcas;
            a.cfg.borrow_mut().marcas = si;
            if a.vivo.get() {
                pintar_todo();
            }
        }
        c::NUEVOS_MD => {
            let si = !a.cfg.borrow().nuevos_md;
            a.cfg.borrow_mut().nuevos_md = si;
            // El documento sin archivo que está abierto también cambia, con lo que lleve escrito.
            if a.ruta.borrow().is_none() && md != si {
                let (s, e) = editor::seleccion(ed);
                cargar(texto(), None, a.cod.get(), a.saltos.get(), a.nulos.get());
                editor::seleccionar(ed, s, e);
                editor::ver_cursor(ed);
                marcar();
            }
        }
        c::LETRA => elegir_letra(),
        c::ACERCAR => zoom(1),
        c::ALEJAR => zoom(-1),
        c::NORMAL => zoom(0),
        n if (c::COD..c::COD + 5).contains(&n) => {
            let nueva = match n - c::COD {
                0 => Cod::Utf8,
                1 => Cod::Utf8Bom,
                2 => Cod::Utf16 { be: false, bom: true },
                3 => Cod::Utf16 { be: true, bom: true },
                _ => Cod::Ansi(fichero::ansi()),
            };
            if nueva != a.cod.get() {
                a.cod.set(nueva);
                marcar();
                estado();
            }
        }
        n if (c::MARGENES..c::MARGENES + 4).contains(&n) => {
            a.cfg.borrow_mut().margenes = n - c::MARGENES;
            margenes();
        }
        n if (c::SALTOS..c::SALTOS + 3).contains(&n) => {
            let nuevos = [Saltos::Crlf, Saltos::Lf, Saltos::Cr][(n - c::SALTOS) as usize];
            if nuevos != a.saltos.get() {
                a.saltos.set(nuevos);
                marcar();
                estado();
            }
        }
        _ => {}
    }
}

// --- Archivo ---

fn es_md(r: Option<&Path>) -> bool {
    let e = r.and_then(Path::extension).map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(e.as_str(), "md" | "markdown" | "mdown" | "mkd" | "mkdn")
}

fn nombre() -> String {
    match app().ruta.borrow().as_deref().and_then(Path::file_name) {
        Some(n) => n.to_string_lossy().into_owned(),
        None => t().sin_titulo.into(),
    }
}

fn hora(r: &Path) -> Option<SystemTime> {
    std::fs::metadata(r).and_then(|m| m.modified()).ok()
}

fn aviso(texto: &str) {
    mensaje(app().hwnd.get(), texto, MB_OK | MB_ICONWARNING);
}

fn confirmar(pregunta: &str) -> bool {
    mensaje(app().hwnd.get(), pregunta, MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) == IDYES
}

fn abrir_inicial(archivo: Option<OsString>) {
    let Some(a) = archivo else {
        if !abrir_nota() {
            cargar(Vec::new(), None, Cod::Utf8, Saltos::Crlf, false);
        }
        return;
    };
    let r = std::path::absolute(&a).unwrap_or_else(|_| PathBuf::from(&a));
    // Si el archivo existe pero no se puede leer, el documento vacío NO se asocia a él: un Ctrl+S lo machacaría.
    if r.exists() {
        if !abrir(&r) {
            cargar(Vec::new(), None, Cod::Utf8, Saltos::Crlf, false);
        }
    } else {
        cargar(Vec::new(), Some(r), Cod::Utf8, Saltos::Crlf, false);
        poner_mensaje(t().archivo_nuevo);
    }
}

fn nuevo() {
    if puedo_descartar() {
        cargar(Vec::new(), None, Cod::Utf8, Saltos::Crlf, false);
    }
}

fn dialogo(guardar: bool) -> Option<PathBuf> {
    let a = app();
    let t = t();
    let actual = a.ruta.borrow().clone();
    let mut buf = vec![0u16; 32768];
    if guardar
        && let Some(n) = actual.as_deref().and_then(Path::file_name) {
            for (i, c) in std::os::windows::ffi::OsStrExt::encode_wide(n).take(buf.len() - 1).enumerate() {
                buf[i] = c;
            }
        }
    let todos = format!("{}\0*.*\0", t.filtro_todos);
    let texto = format!("{}\0*.txt\0", t.filtro_texto);
    let markdown = "Markdown (*.md)\0*.md;*.markdown\0";
    let filtro = ws(&if guardar { format!("{texto}{markdown}{todos}") } else { format!("{todos}{texto}{markdown}") });
    let dir = actual.as_deref().and_then(Path::parent).map(|d| crate::w::ws_os(d.as_os_str()));
    let md = a.md.get();
    let ext = ws(if md { "md" } else { "txt" });
    let mut o: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    o.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    o.hwndOwner = a.hwnd.get();
    o.lpstrFilter = filtro.as_ptr();
    o.nFilterIndex = if guardar && md { 2 } else { 1 };
    o.lpstrFile = buf.as_mut_ptr();
    o.nMaxFile = buf.len() as u32;
    o.lpstrInitialDir = dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
    // OFN_NOCHANGEDIR: si no, la carpeta del archivo queda como la de trabajo de Bloc y Windows no deja cambiarle el
    // nombre ni moverla mientras esté abierto.
    o.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR | if guardar { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST };
    if guardar {
        o.lpstrDefExt = ext.as_ptr();
    }
    let ok = unsafe { if guardar { GetSaveFileNameW(&mut o) } else { GetOpenFileNameW(&mut o) } };
    if ok == 0 {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(crate::w::ruta(&buf[..n]))
}

fn abrir_dialogo() {
    if puedo_descartar()
        && let Some(r) = dialogo(false) {
            abrir(&r);
        }
}

fn nombre_cod(c: Cod) -> String {
    match c {
        Cod::Utf8 => "UTF-8".into(),
        Cod::Utf8Bom => t().utf8_bom.into(),
        Cod::Utf16 { be, bom } => format!("UTF-16 {}{}", if be { "BE" } else { "LE" }, if bom { "" } else { t().sin_bom }),
        Cod::Ansi(1252) => "ANSI".into(),
        Cod::Ansi(cp) => format!("ANSI ({cp})"),
    }
}

fn nombre_saltos(s: Saltos) -> &'static str {
    match s {
        Saltos::Crlf => t().crlf,
        Saltos::Lf => t().lf,
        Saltos::Cr => t().cr,
    }
}

pub fn abrir(r: &Path) -> bool {
    let a = app();
    // La hora, antes de leer: si otro lo guarda mientras se lee, al guardar aquí se avisará.
    let escrito = hora(r);
    let l = match fichero::leer(r) {
        Ok(l) => l,
        Err(e) => {
            aviso(&format!("{}\n\n{e}", con(t().no_abrir, r.display())));
            return false;
        }
    };
    cargar(l.texto, Some(r.to_path_buf()), l.cod, l.saltos, l.nulos);
    a.invalidos.set(matches!(l.aviso, Aviso::BytesNoValidos));
    a.escrito.set(escrito);
    let t = t();
    let msg = match l.aviso {
        Aviso::Nulos => t.aviso_nulos.to_string(),
        Aviso::BytesNoValidos => con(t.aviso_bytes, nombre_cod(l.cod)),
        Aviso::Mezclados => con(t.aviso_mezclados, nombre_saltos(l.saltos)),
        Aviso::Ninguno if a.raro.get() => t.aviso_raro.to_string(),
        Aviso::Ninguno => String::new(),
    };
    poner_mensaje(&msg);
    poner_hora_si_log();
    crate::config::poner_reciente(r);
    true
}

// Como el Bloc de notas: si el archivo empieza por .LOG, al abrirlo se apunta la hora al final.
fn poner_hora_si_log() {
    let ed = app().ed.get();
    let inicio = editor::rango(ed, 0, 5);
    let log: Vec<u16> = ".LOG".encode_utf16().collect();
    if inicio.len() < 4 || inicio[..4] != log[..] || inicio.len() == 5 && inicio[4] != CR {
        return;
    }
    let n = editor::largo(ed);
    let mut s = if editor::rango(ed, n.saturating_sub(1), n) == [CR] { Vec::new() } else { vec![CR] };
    s.extend(fecha());
    s.push(CR);
    editor::seleccionar(ed, n, n);
    editor::sustituir(ed, &s);
    editor::ver_cursor(ed);
}

/// Hora y fecha como las escribe Windows en este idioma.
fn fecha() -> Vec<u16> {
    let (h, d) = hora_y_dia(None);
    [h, vec![b' ' as u16], d].concat()
}

/// Día y hora de un momento, en la hora de aquí: «12/11/2026 14:30».
fn fecha_de(t: SystemTime) -> Vec<u16> {
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let n = t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() / 100) as u64 + 116_444_736_000_000_000;
    let ft = FILETIME { dwLowDateTime: n as u32, dwHighDateTime: (n >> 32) as u32 };
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut aqui: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc);
        SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut aqui);
    }
    let (h, d) = hora_y_dia(Some(&aqui));
    [d, vec![b' ' as u16], h].concat()
}

/// La hora y el día de `cuando` (de ahora, si no se dice), como los escribe Windows en este idioma.
fn hora_y_dia(cuando: Option<&SYSTEMTIME>) -> (Vec<u16>, Vec<u16>) {
    use windows_sys::Win32::Globalization::{GetDateFormatEx, GetTimeFormatEx, DATE_SHORTDATE, TIME_NOSECONDS};
    let p = cuando.map_or(std::ptr::null(), |c| c as *const SYSTEMTIME);
    let mut h = [0u16; 80];
    let mut d = [0u16; 80];
    unsafe {
        let nh = GetTimeFormatEx(std::ptr::null(), TIME_NOSECONDS, p, std::ptr::null(), h.as_mut_ptr(), 80).max(1) as usize - 1;
        let nd = GetDateFormatEx(std::ptr::null(), DATE_SHORTDATE, p, std::ptr::null(), d.as_mut_ptr(), 80, std::ptr::null()).max(1) as usize - 1;
        (h[..nh].to_vec(), d[..nd].to_vec())
    }
}

fn cargar(texto: Vec<u16>, ruta: Option<PathBuf>, cod: Cod, saltos: Saltos, nulos: bool) {
    let md = es_md(ruta.as_deref()) || (ruta.is_none() && app().cfg.borrow().nuevos_md);
    cargar_como(texto, ruta, cod, saltos, nulos, md);
}

fn cargar_como(texto: Vec<u16>, ruta: Option<PathBuf>, cod: Cod, saltos: Saltos, nulos: bool, md: bool) {
    let a = app();
    let ed = a.ed.get();
    crate::copia::borrar(a.hwnd.get());
    let vivo = md && texto.len() <= LIMITE_VIVO;
    a.vivo.set(vivo);
    aplicando(|| {
        editor::deshacer_nativo(ed, !vivo);
        // Primero se vacía: dar formato al texto que se va a quitar cuesta segundos si es grande.
        editor::poner_texto(ed, &[]);
        aplicar_formato_base();
        // Antes del texto: cambiarlos después obliga a colocarlo otra vez.
        margenes();
        a.forzado.set(None);
        editor::poner_texto(ed, &texto);
        // Poner el texto devuelve el zoom al 100 %: se pone otra vez el de «Ver».
        let z = a.cfg.borrow().zoom;
        if z != 100 {
            editor::zoom(ed, z);
        }
    });
    // Los saltos blandos van en el control como md::JUNTO: un archivo que ya trae alguno no se junta.
    editor::juntar(vivo && !texto.contains(&md::JUNTO));
    let leido = editor::texto(ed);
    a.raro.set(leido != texto);
    a.historial.borrow_mut().limpiar();
    a.tabla_cursor.set(None);
    a.abiertas.borrow_mut().clear();
    a.disenos.borrow_mut().1.clear();
    a.marca.set((0, 1, 0));
    a.sel_antes.set(0);
    *a.nuevo.borrow_mut() = Vec::new();
    // Antes de pintar: las imágenes de un .md se buscan en su carpeta.
    *a.ruta.borrow_mut() = ruta;
    soltar_imagenes();
    if vivo {
        a.hay_vallas.set(md::hay_valla(&leido));
        a.vallas_en.borrow_mut().clear();
        *a.copia.borrow_mut() = leido;
        pintar_todo();
    } else {
        *a.copia.borrow_mut() = Vec::new();
    }
    editor::seleccionar(ed, 0, 0);
    a.cod.set(cod);
    a.saltos.set(saltos);
    a.nulos.set(nulos);
    a.invalidos.set(false);
    a.cambiado.set(false);
    a.escrito.set(None);
    modo_md(md);
    titulo();
    poner_mensaje("");
    estado();
}

// La letra y los colores de base: los del texto plano o los de los Markdown, sin que entren en el deshacer.
fn aplicar_formato_base() {
    let a = app();
    let ed = a.ed.get();
    let noche = en_oscuro();
    let cfg = a.cfg.borrow();
    // Sin avisos: cambiar el formato no es cambiar el documento.
    editor::quieto(ed, || {
        if a.vivo.get() {
            let c = tema::colores_md(noche);
            editor::fondo(ed, c.fondo);
            editor::sin_deshacer(ed, || editor::formato_plano(ed, "Segoe UI", 11, c.texto));
        } else {
            let p = tema::paleta(noche);
            editor::fondo(ed, p.fondo);
            editor::sin_deshacer(ed, || editor::formato_plano(ed, &cfg.letra, cfg.tamano, p.texto));
        }
    });
}

// Mientras se imprime: los puntos por pulgada de la impresora de referencia y el ancho del texto en ellos.
thread_local!(static IMPRIMIENDO: Cell<Option<(i32, i32)>> = const { Cell::new(None) });

fn imprimiendo() -> bool {
    IMPRIMIENDO.get().is_some()
}

/// El texto se pinta oscuro con el modo noche, salvo mientras se imprime (sin tocar la configuración, que se guarda).
fn en_oscuro() -> bool {
    app().cfg.borrow().noche && !imprimiendo()
}

/// Para imprimir, el documento en claro, sin las marcas de Markdown y con las tablas plegadas a la medida del papel
/// (`papel`: los puntos por pulgada de la impresora y el ancho del texto en ellos); con None, como estaba.
pub fn para_imprimir(papel: Option<(i32, i32)>) {
    let a = app();
    IMPRIMIENDO.set(papel);
    aplicar_formato_base();
    if a.vivo.get() {
        pintar_todo();
    }
}

/// A medias de una edición o de dar formato: lo que llegue de fuera (las páginas para imprimir) espera.
pub fn ocupada() -> bool {
    let a = app();
    a.aplicando.get() || a.en_accion.get() || editor::en_curso()
}

pub fn titulo_documento() -> String {
    nombre()
}

fn modo_md(md: bool) {
    let a = app();
    a.md.set(md);
    unsafe { ShowWindow(a.botones.get(), if md { SW_SHOWNA } else { SW_HIDE }) };
    colocar();
}

/// `de_fondo`: si se guarda en el mismo archivo, se escribe aparte y la ventana sigue a lo suyo (Ctrl+S). Si no,
/// se espera al resultado, que hace falta para seguir (al cerrar con cambios, Guardar como).
fn guardar(como: bool, de_fondo: bool) -> bool {
    let a = app();
    let t = t();
    if a.guardando.borrow().is_some() {
        if de_fondo {
            a.otra_vez.set(true);
            return true;
        }
        esperar_guardado();
    }
    let mut r = a.ruta.borrow().clone();
    if como || r.is_none() {
        r = dialogo(true);
    }
    let Some(r) = r else { return false };
    if a.nulos.get() && !confirmar(&format!("{}\n\n{}", t.nulos_guardar, t.guardar_igual)) {
        return false;
    }
    if a.raro.get() && !confirmar(&format!("{}\n\n{}", t.raro_guardar, t.guardar_igual)) {
        return false;
    }
    if a.invalidos.get() && !confirmar(&format!("{}\n\n{}", con(t.aviso_bytes, nombre_cod(a.cod.get())), t.guardar_igual)) {
        return false;
    }
    // También si no existía al abrirlo (p. ej., el NAS estaba desconectado) y ahora sí.
    let misma = a.ruta.borrow().as_deref() == Some(r.as_path());
    let ahora = hora(&r);
    // La nota de la última vez, en su archivo: si ya no está, con otro nombre; si ha cambiado desde que se dejó, se
    // pregunta qué hacer.
    let de_nota = misma && a.nota.borrow().is_some() && a.escrito.get().is_some();
    if de_nota && ahora.is_none() {
        return guardar(true, de_fondo);
    }
    if de_nota && ahora != a.escrito.get() {
        let n = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match dialogo::preguntar(&con(t.cambiado_nota, n), &[(SUSTITUIR, t.b_sustituir), (OTRO_NOMBRE, t.b_otro_nombre), (IDCANCEL, t.cancelar)], None) {
            SUSTITUIR => {}
            OTRO_NOMBRE => return guardar(true, de_fondo),
            _ => return false,
        }
    } else if misma && ahora.is_some() && ahora != a.escrito.get() {
        let n = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let p = if a.escrito.get().is_none() { con(t.no_existia, &n) } else { con(t.cambiado_fuera, &n) };
        if !confirmar(&format!("{p}\n\n{}", t.guardar_igual)) {
            return false;
        }
    }
    let texto = texto();
    let (rev, cod, saltos) = (a.rev.get(), a.cod.get(), a.saltos.get());
    if de_fondo && misma {
        poner_mensaje(t.guardando);
        let h = a.hwnd.get() as isize; // HWND no se puede pasar a otro hilo; su número, sí
        let ronda = a.ronda.get() + 1;
        a.ronda.set(ronda);
        let hilo = std::thread::spawn(move || {
            // El aviso a la ventana sale siempre, también si algo falla a medias: si no, se quedaría esperándolo.
            struct Avisar(isize, usize);
            impl Drop for Avisar {
                fn drop(&mut self) {
                    unsafe { PostMessageW(self.0 as HWND, WM_GUARDADO, self.1, 0) };
                }
            }
            let _avisar = Avisar(h, ronda);
            let resultado = fichero::escribir(&r, &texto, cod, saltos);
            Guardado { hora: hora(&r), ruta: r, rev, cod, resultado }
        });
        *a.guardando.borrow_mut() = Some(hilo);
        return true;
    }
    let nueva = (!misma).then(|| r.clone());
    let resultado = fichero::escribir(&r, &texto, cod, saltos);
    let cambia_tipo = resultado.is_ok() && es_md(Some(&r)) != a.md.get();
    if cambia_tipo {
        // De .txt a .md o al revés: el editor cambia de forma y el deshacer empieza de nuevo.
        let (s, e) = editor::seleccion(a.ed.get());
        cargar(texto, Some(r.clone()), cod, saltos, false);
        editor::seleccionar(a.ed.get(), s, e);
        editor::ver_cursor(a.ed.get());
    }
    let bien = tras_guardar(Guardado { hora: hora(&r), ruta: r, rev: a.rev.get(), cod, resultado });
    if bien && cambia_tipo {
        poner_mensaje(t.deshacer_nuevo);
    }
    if bien && let Some(r) = nueva {
        crate::config::poner_reciente(&r);
    }
    bien
}

/// Avisa si no se ha podido escribir o da el documento por guardado (si no ha cambiado mientras).
fn tras_guardar(g: Guardado) -> bool {
    let a = app();
    let t = t();
    let usada = match g.resultado {
        Ok(u) => u,
        Err(e) => {
            let detalle = match e {
                fichero::ErrorGuardar::SoloLectura => t.solo_lectura.to_string(),
                fichero::ErrorGuardar::Io(e) => e.to_string(),
                fichero::ErrorGuardar::Apartado { original, nuevo } => {
                    con(&con(t.apartado, original.display()), nuevo.display())
                }
            };
            poner_mensaje("");
            aviso(&format!("{}\n\n{detalle}", con(t.no_guardar, g.ruta.display())));
            return false;
        }
    };
    *a.ruta.borrow_mut() = Some(g.ruta);
    // Si mientras se guardaba se ha elegido otra codificación, se queda la elegida.
    if a.cod.get() == g.cod {
        a.cod.set(usada);
    }
    if a.rev.get() == g.rev {
        a.cambiado.set(false);
    }
    a.nulos.set(false);
    a.raro.set(false);
    a.invalidos.set(false);
    a.escrito.set(g.hora);
    poner_mensaje(if usada != g.cod { t.guardado_utf8 } else { t.guardado });
    titulo();
    estado();
    decidido();
    // Si se ha seguido escribiendo mientras se guardaba, la copia sigue haciendo falta.
    if !a.cambiado.get() {
        crate::copia::borrar(a.hwnd.get());
    }
    true
}

/// Si se está guardando aparte, espera a que acabe y lo da por hecho. Devuelve si hay que volver a guardar: se pidió
/// otra vez mientras y aún quedan cambios.
fn esperar_guardado() -> bool {
    let a = app();
    let Some(hilo) = a.guardando.borrow_mut().take() else { return false };
    let bien = match hilo.join() {
        Ok(g) => tras_guardar(g),
        Err(_) => {
            poner_mensaje("");
            aviso(&con(t().no_guardar, nombre()));
            false
        }
    };
    a.otra_vez.replace(false) && bien && a.cambiado.get()
}

fn puedo_descartar() -> bool {
    let a = app();
    esperar_guardado();
    if !a.cambiado.get() {
        return true;
    }
    match mensaje(a.hwnd.get(), &con(t().guardar_cambios, nombre()), MB_YESNOCANCEL | MB_ICONWARNING) {
        IDYES => guardar(false, false),
        IDNO => {
            decidido();
            true
        }
        _ => false,
    }
}

/// Al cerrar con cambios: guardar, no guardar, seguir la próxima vez o cancelar.
fn puedo_cerrar() -> bool {
    let a = app();
    esperar_guardado();
    if !a.cambiado.get() {
        return true;
    }
    let t = t();
    let botones = [(GUARDAR, t.b_guardar), (NO_GUARDAR, t.b_no_guardar), (SEGUIR, t.b_seguir), (IDCANCEL, t.cancelar)];
    match dialogo::preguntar(&con(t.guardar_cambios, nombre()), &botones, None) {
        GUARDAR => guardar(false, false),
        NO_GUARDAR => {
            decidido();
            true
        }
        SEGUIR => seguir(true),
        _ => false,
    }
}

/// Lo de ahora, tal cual, para la nota.
fn nota_de_ahora() -> Nota {
    let a = app();
    let ed = a.ed.get();
    let primera = enviar(ed, EM_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize;
    Nota {
        texto: texto(),
        ruta: a.ruta.borrow().clone(),
        cod: a.cod.get(),
        saltos: a.saltos.get(),
        escrito: a.escrito.get(),
        md: a.md.get(),
        sel: editor::seleccion(ed),
        arriba: enviar(ed, EM_LINEINDEX, primera, 0).max(0) as usize,
        fecha: SystemTime::now(),
        serie: 0,
    }
}

/// La nota es la que enseña esta ventana, sin nada de otra añadido después.
fn mia(n: &Nota) -> bool {
    app().nota.borrow().as_ref().is_some_and(|(_, s)| *s == n.serie)
}

/// Guardado o descartado lo de la nota: ya no hace falta para la próxima vez. Si otra ventana le ha añadido lo suyo,
/// se queda.
fn decidido() {
    let a = app();
    if a.nota.borrow().is_none() {
        return;
    }
    if let Some(_cerrojo) = nota::cerrojo()
        && nota::leer().is_some_and(|n| mia(&n))
    {
        nota::borrar();
    }
    a.nota.replace(None);
}

/// La página de qué abre Bloc, la vista previa y el inicio con Windows (eleccion.rs), desde Archivo o, la primera vez
/// que se abre tras ponerse al día desde la 1.1, sola y una vez: entonces Cancelar deja todo como estaba (las 14
/// extensiones en «Abrir con», sin vista previa ni inicio con Windows) y no vuelve a salir.
fn elegir(primera: bool) {
    let nombre = ws("Local\\Bloc.Elegir");
    // Una sola a la vez entre todas las ventanas.
    let cerrojo = unsafe { CreateMutexW(std::ptr::null(), 1, nombre.as_ptr()) };
    if cerrojo.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(cerrojo) };
        return;
    }
    let (mut e, guardada) = crate::instalar::eleccion();
    if !(primera && guardada) {
        let noche = app().cfg.borrow().noche;
        let ahora = crate::instalar::ahora(&e.todas);
        match crate::eleccion::preguntar(app().hwnd.get(), noche, &mut e, &ahora) {
            IDOK => {
                crate::instalar::aplicar(&e);
                crate::instalar::menu_corto(e.editar);
            }
            _ if primera => crate::instalar::aplicar(&crate::instalar::Eleccion {
                abrir: crate::instalar::EXTENSIONES.iter().map(|x| x.to_string()).collect(),
                vista: Vec::new(),
                inicio: false,
                editar: false,
                todas: e.todas,
            }),
            _ => {}
        }
    }
    unsafe {
        ReleaseMutex(cerrojo);
        CloseHandle(cerrojo);
    }
}

/// «Seguir la próxima vez»: el documento, tal cual, a la nota. Si ya hay una de otra ventana o de otro día, se añade al
/// final de esa, preguntando antes (al apagar, sin preguntar). Si no se puede escribir, se dice (`avisar`) y no se
/// cierra: el texto nunca se pierde.
fn seguir(avisar: bool) -> bool {
    let a = app();
    let t = t();
    let fallo = |e: std::io::Error| {
        if avisar {
            aviso(&con(t.no_seguir, e));
        }
        false
    };
    let ocupada = || std::io::Error::from_raw_os_error(32); // ERROR_SHARING_VIOLATION, con el texto de Windows
    let mut n = nota_de_ahora();
    let Some(mut cerrojo) = nota::cerrojo() else { return fallo(ocupada()) };
    let mut otra = nota::leer().filter(|v| !mia(v));
    if let Some(v) = otra.as_ref().filter(|_| avisar) {
        // Sin el cerrojo mientras se pregunta: las demás ventanas no tienen que esperar a que se conteste.
        drop(cerrojo);
        let fecha = String::from_utf16_lossy(&fecha_de(v.fecha));
        let pregunta = con(&con(t.ya_hay_nota, fecha), nota::primera_linea(&v.texto));
        if dialogo::preguntar(&pregunta, &[(ANADIR, t.b_anadir), (IDCANCEL, t.b_volver)], None) != ANADIR {
            return false;
        }
        let Some(c) = nota::cerrojo() else { return fallo(ocupada()) };
        cerrojo = c;
        // Otra vez: mientras se contestaba, otra ventana ha podido cambiarla.
        otra = nota::leer().filter(|v| !mia(v));
    }
    let propia = otra.is_none();
    match otra {
        Some(v) => n = nota::anadir(&v, n),
        None => n.serie = nota::leer().map_or(0, |v| v.serie) + 1,
    }
    if let Err(e) = nota::escribir(&n) {
        return fallo(e);
    }
    drop(cerrojo);
    crate::copia::borrar(a.hwnd.get());
    // Si era la suya o no había, la tiene esta ventana: si al final no se cierra (un apagado que se cancela), sigue
    // siendo la suya.
    let tenencia = a.nota.borrow_mut().take().map(|(t, _)| t);
    if propia {
        *a.nota.borrow_mut() = tenencia.or_else(nota::tomar).map(|t| (t, n.serie));
    }
    true
}

/// La nota de la última vez en esta ventana, si no la tiene otra: como estaba, marcada como cambiada y con su archivo,
/// así que Ctrl+S guarda en él.
fn abrir_nota() -> bool {
    let a = app();
    let Some(tenencia) = nota::tomar() else { return false };
    let Some(n) = nota::leer() else { return false };
    let ed = a.ed.get();
    cargar_como(n.texto, n.ruta, n.cod, n.saltos, false, n.md);
    a.cambiado.set(true);
    a.escrito.set(n.escrito);
    *a.nota.borrow_mut() = Some((tenencia, n.serie));
    editor::seleccionar(ed, n.sel.0, n.sel.1);
    let linea = enviar(ed, EM_LINEFROMCHAR, n.arriba, 0);
    let primera = enviar(ed, EM_GETFIRSTVISIBLELINE, 0, 0);
    enviar(ed, EM_LINESCROLL, 0, linea - primera);
    titulo();
    poner_mensaje(t().nota_ultima);
    estado();
    true
}

fn guardar_config() {
    let a = app();
    // La vista previa tiene la suya (sin barra, al 100 %...): nunca va al bloc.ini.
    if a.vista.get() {
        return;
    }
    let h = a.hwnd.get();
    let mut p: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    p.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    unsafe { GetWindowPlacement(h, &mut p) };
    let r = p.rcNormalPosition;
    let max = unsafe { IsZoomed(h) } != 0;
    // La ventana se va ya: escribir las preferencias puede tener que esperar al disco. Antes de tomar la
    // configuración: al ocultarse llegan mensajes (WM_NCACTIVATE) que la leen.
    unsafe { ShowWindow(h, SW_HIDE) };
    let mut cfg = a.cfg.borrow_mut();
    if let Some(n) = a.noche_guardada.get() {
        cfg.noche = n;
    }
    cfg.max = max;
    cfg.marco = Some((r.left, r.top, r.right - r.left, r.bottom - r.top));
    cfg.guardar();
}

pub fn soltar(d: HDROP) {
    let mut b = vec![0u16; 32768];
    let n = unsafe { DragQueryFileW(d, 0, b.as_mut_ptr(), b.len() as u32) } as usize;
    unsafe { DragFinish(d) };
    if n > 0 && puedo_descartar() {
        abrir(&crate::w::ruta(&b[..n]));
    }
}

// --- Texto y deshacer ---

/// Todo el texto, con \r por salto.
pub fn texto() -> Vec<u16> {
    let a = app();
    if a.vivo.get() { a.copia.borrow().clone() } else { editor::texto(a.ed.get()) }
}

/// Un trozo del texto, sin pedirle todo al control.
pub fn trozo(ini: usize, fin: usize) -> Vec<u16> {
    let a = app();
    if a.vivo.get() {
        let c = a.copia.borrow();
        c[ini.min(c.len())..fin.min(c.len())].to_vec()
    } else {
        editor::rango(a.ed.get(), ini, fin)
    }
}

pub fn largo() -> usize {
    let a = app();
    if a.vivo.get() { a.copia.borrow().len() } else { editor::largo(a.ed.get()) }
}

fn inicio_linea(t: &[u16], pos: usize) -> usize {
    t[..pos.min(t.len())].iter().rposition(|&c| c == CR).map_or(0, |i| i + 1)
}

fn fin_linea(t: &[u16], pos: usize) -> usize {
    let p = pos.min(t.len());
    t[p..].iter().position(|&c| c == CR).map_or(t.len(), |i| p + i)
}

/// La vista previa del Explorador: solo para ver.
pub fn solo_ver() -> bool {
    app().vista.get()
}

pub fn deshacer_propio() -> bool {
    app().vivo.get()
}

/// Lo que se pega de unos archivos copiados en el Explorador, uno por línea y en su orden. En un .md (con formato), un
/// enlace a cada uno, `![nombre](ruta)` si es una imagen; la ruta, relativa al .md si está en su carpeta o debajo, y
/// escrita como en un enlace de HTML pegado. Si no, su ruta completa. Nada se copia ni se mueve.
pub fn archivos_al_pegar(rutas: &[Vec<u16>], con_formato: bool) -> Vec<u16> {
    use std::os::windows::ffi::OsStringExt;
    let a = app();
    let carpeta = a.ruta.borrow().as_ref().and_then(|r| r.parent().map(std::path::Path::to_path_buf));
    let lineas: Vec<Vec<u16>> = rutas
        .iter()
        .map(|r| {
            if !con_formato {
                return r.clone();
            }
            let ruta = PathBuf::from(OsString::from_wide(r));
            let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let destino = match carpeta.as_ref().and_then(|c| ruta.strip_prefix(c).ok()) {
                Some(dentro) => dentro.to_string_lossy().replace('\\', "/"),
                None => ruta.to_string_lossy().replace('\\', "/"),
            };
            let ext = ruta.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let imagen = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico");
            let nombre = nombre.replace('[', "\\[").replace(']', "\\]");
            format!("{}[{nombre}]({})", if imagen { "!" } else { "" }, crate::html::url(&destino.replace('%', "%25"))).encode_utf16().collect()
        })
        .collect();
    lineas.join(&CR)
}

/// Una imagen pegada (M5): en un .md, guardada como PNG junto a él y enlazada donde está el cursor; un .md sin guardar
/// pide antes dónde guardarlo (si se cancela, nada). En lo demás, solo un aviso. Deshacer quita el enlace, no el archivo.
pub fn imagen_al_pegar(im: &crate::imagen::Imagen) {
    let a = app();
    let t = t();
    if !a.md.get() {
        poner_mensaje(t.imagen_solo_md);
        return;
    }
    if a.ruta.borrow().is_none() && !guardar(true, false) {
        return;
    }
    let Some(ruta) = a.ruta.borrow().clone() else { return };
    match crate::imagen::guardar_junto(&ruta, im) {
        Ok(nombre) => {
            let enlace: Vec<u16> = format!("![]({})", crate::html::url(&nombre.replace('%', "%25"))).encode_utf16().collect();
            cortar_deshacer();
            editor::editando(|| editor::sustituir(a.ed.get(), &enlace));
            cortar_deshacer();
            poner_mensaje(&con(t.imagen_guardada, &nombre));
        }
        Err(crate::imagen::Fallo::Grande) => poner_mensaje(t.imagen_grande),
        Err(crate::imagen::Fallo::NoSeEscribe) => poner_mensaje(t.imagen_no_se_guarda),
    }
}

/// Lo copiado con formato se pega convertido en Markdown en los .md, salvo dentro de un bloque de código.
pub fn pegar_con_formato() -> bool {
    let a = app();
    if !a.md.get() {
        return false;
    }
    if a.vivo.get() && !a.hay_vallas.get() {
        return true;
    }
    let (s, _) = editor::seleccion(a.ed.get());
    let antes = trozo(0, s);
    md::estado_hasta(&antes, inicio_linea(&antes, s)).is_none()
}

/// Una acción de Bloc (un botón, Tab, Intro, Retroceso): deshacerla devuelve la selección de antes y rehacerla, la de
/// después, como Word; lo cambiado no tiene por qué coincidir con lo seleccionado.
pub fn accion<R>(f: impl FnOnce() -> R) -> R {
    let a = app();
    if !a.vivo.get() || a.en_accion.get() {
        return f();
    }
    let ed = a.ed.get();
    let antes = editor::seleccion(ed);
    let desde = a.historial.borrow().anotados();
    recordar_tabla();
    a.en_accion.set(true);
    let r = {
        let _fin = editor::AlSalir(|| a.en_accion.set(false));
        f()
    };
    if a.vivo.get() {
        a.historial.borrow_mut().selecciones(desde, antes, editor::seleccion(ed));
    }
    mirar_tabla();
    mirar_tablas();
    tapar(true);
    r
}

/// Tras saltar a otro sitio (buscar, ir a una línea), su tabla se abre antes de llevar la vista hasta él: abierta
/// después, más alta, dejaba lo encontrado fuera de la vista.
pub fn ver_cursor() {
    let a = app();
    let en = a.en_accion.replace(false);
    mirar_tablas();
    a.en_accion.set(en);
    editor::ver_cursor(a.ed.get());
}

/// Dónde empieza la tabla en que está el cursor, si está en una.
fn tabla_del_cursor() -> Option<usize> {
    let a = app();
    let s = editor::seleccion(a.ed.get()).0;
    let c = a.copia.borrow();
    let p = s.min(c.len());
    tabla_en(&c, inicio_linea(&c, p)).map(|x| x.0)
}

/// Apunta en qué tabla está el cursor antes de una tecla, un clic o una acción: si tras ella está fuera, `mirar_tabla`
/// la alinea.
pub fn recordar_tabla() {
    let a = app();
    if a.vivo.get() && !a.en_accion.get() {
        a.tabla_cursor.set(tabla_del_cursor());
    }
}

/// Si el cursor ha salido de la tabla en que estaba, la alinea, en un paso de deshacer aparte: lo del botón Alinear, sin
/// tener que pulsarlo. Se mira tras cada tecla, clic o acción de Bloc, nunca en un aviso de selección (ahí no se puede
/// cambiar el texto) ni con el botón del ratón pulsado. Decidido por Claude el 10/11, se puede cambiar.
pub fn mirar_tabla() {
    let a = app();
    if a.vista.get() || !a.vivo.get() || a.aplicando.get() || a.en_accion.get() || editor::en_curso() || pulsada(VK_LBUTTON) {
        return;
    }
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let ahora = tabla_del_cursor();
    let Some(d) = a.tabla_cursor.replace(ahora).filter(|&d| ahora != Some(d)) else { return };
    let t = a.copia.borrow().clone();
    let Some((desde, hasta, _)) = tabla_en(&t, inicio_linea(&t, d.min(t.len()))).filter(|x| x.0 == d && abierto_en(&t, d).is_none()) else {
        return;
    };
    if s <= hasta && e >= desde {
        return;
    }
    let Some(nuevo) = md::alinear_tablas(&t[desde..hasta]).filter(|n| n[..] != t[desde..hasta]) else { return };
    accion(|| {
        cortar_deshacer();
        cambiar(desde, &t[desde..hasta], &nuevo);
        cortar_deshacer();
        let mover = |p: usize| if p > hasta { p + nuevo.len() - (hasta - desde) } else { p };
        editor::seleccionar(ed, mover(s), mover(e));
    });
}

/// Cambia el control sin que Bloc lo lea como un cambio del usuario. `aplicando` vuelve a false aunque algo falle por el
/// camino: si se quedara puesto, el control y la copia irían cada uno por su lado.
fn aplicando(f: impl FnOnce()) {
    let a = app();
    a.aplicando.set(true);
    let _fin = editor::AlSalir(|| a.aplicando.set(false));
    f();
}

/// Lo que se haga ahora se deshace aparte de lo anterior y de lo siguiente.
pub fn cortar_deshacer() {
    let a = app();
    if a.vivo.get() {
        a.historial.borrow_mut().cortar();
    } else {
        enviar(a.ed.get(), re::EM_STOPGROUPTYPING, 0, 0);
    }
}

pub fn al_cambiar() {
    let a = app();
    if a.aplicando.get() || a.vista.get() {
        return;
    }
    if a.vivo.get() {
        procesar(true);
    } else {
        let (s, _) = editor::seleccion(a.ed.get());
        a.marca.set(tras_cambio(a.marca.get(), s.min(a.sel_antes.get())));
        buscar::cambiado();
    }
    poner_mensaje("");
    marcar();
    estado();
    pulsados();
}

pub fn al_mover() {
    let a = app();
    if a.aplicando.get() || editor::en_curso() {
        return;
    }
    if !a.vivo.get() {
        a.sel_antes.set(editor::seleccion(a.ed.get()).0);
    }
    estado();
    pulsados();
    let (s, e) = editor::seleccion(a.ed.get());
    if editor::destapados(|d| d.iter().any(|&(x, y)| s < x || e > y)) {
        unsafe { PostMessageW(a.hwnd.get(), WM_TAPAR, 0, 0) };
    }
    // Se abren o cierran después: en el aviso de selección RichEdit está a medias y no se le cambia el formato.
    if con_tablas() && !a.tablas_avisadas.get() {
        let ahora = abiertas_en(&a.copia.borrow(), editor::seleccion(a.ed.get()));
        let abiertas = a.abiertas.borrow();
        if ahora.iter().flatten().any(|d| !abiertas.contains(d)) || abiertas.iter().any(|d| !ahora.contains(&Some(*d))) {
            a.tablas_avisadas.set(true);
            unsafe { PostMessageW(a.hwnd.get(), WM_TABLAS, 0, 0) };
        }
    }
}

// Con más seleccionado no se mira qué formato tiene: lo pagaría cada movimiento del cursor.
const MAX_PULSADOS: usize = 20_000;
const PASO_VALLAS: usize = 16_384;

/// Los botones del formato que ya tiene lo seleccionado (o lo que rodea al cursor) se ven pulsados: pulsarlos lo quita.
pub fn pulsados() {
    let a = app();
    let mut ids = Vec::new();
    if buscar::abierto() {
        ids.push(c::BUSCAR);
    }
    if a.vivo.get() {
        let (s, e) = editor::seleccion(a.ed.get());
        let t = a.copia.borrow();
        let e = e.min(t.len());
        let s = s.min(e);
        // Los extremos de las líneas se buscan como mucho hasta ahí: en una línea larguísima, recorrerla entera costaría
        // en cada movimiento del cursor.
        let (desde, hasta) = (s.saturating_sub(MAX_PULSADOS), (e + MAX_PULSADOS).min(t.len()));
        let ini = t[desde..s].iter().rposition(|&c| c == CR).map(|k| desde + k + 1).or((desde == 0).then_some(0));
        let fin = t[e..hasta].iter().position(|&c| c == CR).map(|k| e + k).or((hasta == t.len()).then_some(hasta));
        if ini.zip(fin).is_some_and(|(i, f)| f - i <= MAX_PULSADOS) {
            let (_, _, lineas) = lineas_md(&t, s, e);
            for (id, efecto) in [(c::NEGRITA, md::NEGRITA), (c::CURSIVA, md::CURSIVA), (c::TACHADO, md::TACHADO), (c::CODIGO, md::CODIGO)] {
                if ya_tienen(&lineas, efecto) == Some(true) {
                    ids.push(id);
                }
            }
            let ls: Vec<&[u16]> = lineas.iter().filter(|x| x.1.con_letra()).map(|x| x.0).collect();
            for (id, nivel) in [(c::TITULO1, 1), (c::TITULO2, 2), (c::TITULO3, 3)] {
                if todas_de_nivel(&ls, nivel) {
                    ids.push(id);
                }
            }
            for (id, tipo) in [(c::LISTA, 0), (c::NUMERADA, 1), (c::CASILLAS, 2), (c::CITA, 3)] {
                if todas_de_tipo(&ls, tipo) {
                    ids.push(id);
                }
            }
        }
    }
    barras::pulsados(a.botones.get(), &ids);
}

// Lee el texto nuevo, mira qué ha cambiado, lo anota y vuelve a pintar esas líneas (y las de después si se ha
// abierto o cerrado un bloque de código).
fn procesar(anotar: bool) {
    let a = app();
    a.encima_previa.set(a.rayas_hechas.take());
    a.tablas_hechas.replace(None);
    let ed = a.ed.get();
    let forzado = a.forzado.take();
    a.pareja.set(None);
    let mut nuevo = a.nuevo.take();
    editor::leer(ed, &mut nuevo);
    let cambio = {
        let copia = a.copia.borrow();
        historial::diferencia(&copia, &nuevo).map(|(p, s)| {
            if anotar {
                a.historial.borrow_mut().anotar(&copia, &nuevo, p, s);
            }
            let (fin_viejo, fin_nuevo) = (copia.len() - s, nuevo.len() - s);
            // Desde la línea de antes: la cabecera de una tabla se pinta según la fila |---| que tenga debajo.
            let li = inicio_linea(&copia, inicio_linea(&copia, p).saturating_sub(1));
            let (mut lv, ln) = (fin_linea(&copia, fin_viejo), fin_linea(&nuevo, fin_nuevo));
            let vallas = a.hay_vallas.get() || md::hay_valla(&nuevo[li..ln]);
            let hasta_el_final = vallas && {
                let antes = abierto_en(&copia, li);
                md::estado_tras(&copia[li..lv], antes) != md::estado_tras(&nuevo[li..ln], antes)
            };
            // Si cambia la lista abierta al final de lo tocado, también las líneas de debajo que la continúan.
            let mut ln = ln;
            // El salto de detrás de lo cambiado puede pasar a ser blando o dejar de serlo, y un título lleva hueco encima
            // según esté en blanco o no la línea de antes: se pinta la línea de después.
            if !a.cfg.borrow().marcas && ln < nuevo.len() {
                ln = fin_linea(&nuevo, ln + 1);
                if lv < copia.len() {
                    lv = fin_linea(&copia, lv + 1);
                }
                // Y la de = de debajo de esa: si la línea de después deja de ser un título (tiene texto encima), sus = se
                // ven.
                if ln < nuevo.len() && md::linea_de_titulo(&nuevo[ln + 1..fin_linea(&nuevo, ln + 1)]) == Some(1) {
                    ln = fin_linea(&nuevo, ln + 1);
                    if lv < copia.len() {
                        lv = fin_linea(&copia, lv + 1);
                    }
                }
            }
            if !a.cfg.borrow().marcas {
                let antes = md::en_lista_hasta(&copia, li);
                let (era, es) = (md::en_lista_tras(&copia[li..lv], antes), md::en_lista_tras(&nuevo[li..ln], antes));
                if era != es {
                    ln = md::fin_de_lista(&nuevo, ln, era, es);
                }
                // Y un |---| justo debajo, que se pliega o no según la fila que tenga encima.
                if ln < nuevo.len() && md::plegable(&nuevo[ln + 1..fin_linea(&nuevo, ln + 1)]) {
                    ln = fin_linea(&nuevo, ln + 1);
                }
                // Y la tabla dibujada que había en lo cambiado o justo debajo: si ha dejado de serlo (unas filas nuevas
                // encima la han juntado con otras), sus filas de fuera siguen plegadas al alto que tenían como tabla
                // dibujada. Las de una abierta no estaban plegadas.
                let abiertas = a.abiertas.borrow();
                for x in [li, inicio_linea(&copia, lv), (lv + 1).min(copia.len())] {
                    if let Some((_, h, _)) = tabla_en(&copia, x).filter(|x| !abiertas.contains(&x.0) && pocas_filas(&copia, x)) {
                        let h = if h >= fin_viejo { h - fin_viejo + fin_nuevo } else { fin_nuevo };
                        ln = ln.max(fin_linea(&nuevo, h.min(nuevo.len())));
                    }
                }
            }
            (p, li, ln, vallas, hasta_el_final)
        })
    };
    let lineas_de = |(x, y): (usize, usize)| {
        let c = a.copia.borrow();
        let y = y.min(c.len());
        (inicio_linea(&c, x.min(y)), fin_linea(&c, y))
    };
    let Some((p, li, ln, vallas, hasta_el_final)) = cambio else {
        a.nuevo.replace(nuevo);
        if let Some(f) = forzado {
            let (li, ln) = lineas_de(f);
            pintar(li, ln);
        } else {
            a.rayas_hechas.set(a.encima_previa.take());
        }
        return;
    };
    let viejo = a.copia.replace(nuevo);
    let crece = a.copia.borrow().len() as isize - viejo.len() as isize;
    for q in a.abiertas.borrow_mut().iter_mut().filter(|q| **q > p) {
        *q = q.saturating_add_signed(crece).max(p);
    }
    editor::destapados(|d| {
        for q in d.iter_mut().flat_map(|(x, y)| [x, y]).filter(|q| **q > p) {
            *q = q.saturating_add_signed(crece).max(p);
        }
    });
    a.nuevo.replace(viejo);
    buscar::cambiado();
    a.hay_vallas.set(vallas);
    a.vallas_en.borrow_mut().retain(|&(q, _)| q <= p);
    a.marca.set(tras_cambio(a.marca.get(), p));
    let (li, ln) = forzado.map(lineas_de).map_or((li, ln), |(x, y)| (li.min(x), ln.max(y)));
    let hasta = if hasta_el_final { a.copia.borrow().len() } else { ln };
    pintar(li, hasta);
}

/// Lo que se va a poner en [ini, fin) se pinta entero aunque la diferencia de textos diga otra cosa (ver `forzado`).
pub fn forzar(ini: usize, fin: usize) {
    let a = app();
    if a.vivo.get() {
        a.forzado.set(Some((ini, fin)));
    }
}

fn aplicar(pos: usize, cuanto: usize, texto: &[u16], sel: Option<(usize, usize)>) {
    let a = app();
    let ed = a.ed.get();
    // Si el historial no cuadra con lo que hay (no debería pasar), mejor no tocar el texto.
    if pos + cuanto > a.copia.borrow().len() {
        a.historial.borrow_mut().limpiar();
        return;
    }
    aplicando(|| {
        editor::seleccionar(ed, pos, pos + cuanto);
        editor::sustituir(ed, texto);
    });
    procesar(false);
    let (s, e) = sel.unwrap_or((pos, pos + texto.len()));
    let n = a.copia.borrow().len();
    editor::seleccionar(ed, s.min(n), e.min(n));
    // Deshacer o rehacer no alinea la tabla de la que sale el cursor: sería un paso más y ya no se podría rehacer.
    a.tabla_cursor.set(tabla_del_cursor());
    editor::ver_cursor(ed);
    poner_mensaje("");
    marcar();
    estado();
}

pub fn deshacer() {
    let a = app();
    if a.vivo.get() {
        let c = a.historial.borrow_mut().deshacer();
        if let Some(c) = c {
            aplicar(c.pos, c.puesto.len(), &c.quitado, c.antes);
        }
    } else {
        a.marca.set((0, 1, 0));
        enviar(a.ed.get(), EM_UNDO, 0, 0);
        estado();
    }
}

fn rehacer() {
    let a = app();
    if a.vivo.get() {
        let c = a.historial.borrow_mut().rehacer();
        if let Some(c) = c {
            aplicar(c.pos, c.quitado.len(), &c.puesto, c.despues);
        }
    } else {
        a.marca.set((0, 1, 0));
        enviar(a.ed.get(), re::EM_REDO, 0, 0);
        estado();
    }
}

// --- Markdown en vivo ---

// Poner un tramo de letra cuesta medio milisegundo. Hasta TRAMOS_SIEMPRE, sale como meter el RTF, que además rehace los
// párrafos de dentro (y RichEdit recoloca las viñetas); a partir de ahí, uno más por cada 20.000 caracteres, lo que
// RichEdit tarda en repasar tras meter RTF si hay listas numeradas (20 ms en 900 KB).
const TRAMOS_SIEMPRE: usize = 8;
const CARACTERES_POR_TRAMO: usize = 20_000;
/// A partir de cuántas líneas se pinta con el control oculto (ver `editor::oculto`); con menos, no compensa.
const OCULTO_DESDE: usize = 1000;

/// Letras que Segoe UI y Consolas tienen: latín, griego, cirílico, puntuación y monedas. Las demás (☐, emoji, chino...)
/// las dibuja otra letra, que RichEdit les busca al meter RTF y no siempre al teclearlas.
fn letra_corriente(c: u16) -> bool {
    c < 0x0530 || (0x2000..0x20D0).contains(&c)
}

/// Vuelve a pintar las líneas copia[ini..fin] (ini, inicio de línea; fin, final de línea). Antes y después
/// comprueba que el control tiene exactamente el texto de la copia: el formato nunca puede cambiar lo escrito.
fn pintar(ini: usize, fin: usize) {
    let a = app();
    a.rayas_hechas.set(None);
    let encima = a.encima_previa.take();
    a.tablas_hechas.replace(None);
    a.reintentos_raya.set(0);
    let ed = a.ed.get();
    let copia = a.copia.borrow();
    let total = copia.len();
    let fin = fin.min(total);
    let ini = ini.min(fin);
    let (ini, fin) = bloques_enteros(ed, &copia, ini, fin);
    let (ini, mut fin, plegadas) = if con_tablas() { plegadas(&copia, ini, fin) } else { (ini, fin, Vec::new()) };
    // Nunca se pinta una última línea vacía al final: RichEdit se comería su salto. A esos saltos, y a los que hay justo
    // antes y después de lo pintado, solo se les pone la letra del texto: un salto escrito o pegado coge la letra de
    // donde cae (la de un título, más alta), y si sustituye a otro igual la diferencia de textos no lo ve.
    let hasta = fin;
    while fin == total && fin > ini && copia[fin - 1] == CR {
        fin -= 1;
    }
    let abierto = abierto_en(&copia, ini);
    let colores = tema::colores_md(en_oscuro());
    let ocultar = imprimiendo() || !a.cfg.borrow().marcas;
    let juntar = ocultar && editor::junta() && !a.desjuntando.get() && a.cfg.borrow().ajuste;
    let pintado = (ini < fin).then(|| md::pintar(&colores, &copia, ini, fin, abierto, ocultar, imprimiendo(), &plegadas, juntar));
    let esperado = copia[ini..fin].to_vec();
    // Lo que tiene que quedar en el control: el texto, con md::JUNTO en los saltos blandos. Los párrafos (su viñeta, su
    // sangría) van donde empieza cada uno: tras un salto que no es blando.
    let blandos: &[usize] = pintado.as_ref().map_or(&[], |p| &p.blandos);
    let mut proyeccion = esperado.clone();
    for &k in blandos {
        proyeccion[k] = md::JUNTO;
    }
    let inicios: Vec<usize> =
        std::iter::once(ini).chain((ini..hasta).filter(|&k| copia[k] == CR && blandos.binary_search(&(k - ini)).is_err()).map(|k| k + 1)).collect();
    // Los saltos de los bordes, con la letra que les pondría el RTF (md::salto: el de una línea en blanco es el hueco
    // entre párrafos), en tramos seguidos de la misma.
    let letra_salto = |k: usize| {
        let l = inicio_linea(&copia, k);
        let m = copia.get(k + 1..).map(|r| &r[..r.iter().position(|&c| c == CR).unwrap_or(r.len())]);
        md::salto(&copia[l..k], m, abierto_apuntando(&copia, l, false).is_some(), ocultar)
    };
    let mut bordes: Vec<(usize, usize, md::Estilo)> = Vec::new();
    for (x, y) in [(ini.saturating_sub(1), ini), (fin, hasta.max(fin + 1).min(total))] {
        for k in x..y {
            let e = letra_salto(k);
            match bordes.last_mut() {
                Some(b) if b.1 == k && b.2 == e => b.1 = k + 1,
                _ => bordes.push((k, k + 1, e)),
            }
        }
    }
    drop(copia);
    let mut ahora = editor::rango(ed, ini, fin);
    let como_queda = ahora == proyeccion;
    editor::como_archivo(&mut ahora);
    if editor::largo(ed) != total || ahora != esperado {
        return; // hay un cambio que aún no se ha leído: lo pintará él
    }
    let mut por_rtf = false;
    // Al descongelar (ver editor::quieto), RichEdit manda repintar la ventana entera, y lo de encima tendría que volver
    // a leer la pantalla (ver editor::colorear_fondo). Se repinta solo lo que ya tenía pendiente y lo pintado aquí, y
    // hasta abajo si lo de después se ha movido. Con el control oculto se repinta entero, y con alguna tabla dibujada a
    // la vista también: si se abre, lo dibujado de ella se quedaría donde RichEdit no repinta.
    let acotar = inicios.len() <= OCULTO_DESDE && !a.tablas_vistas.get();
    let pendiente = acotar.then(|| (editor::pendiente(ed), editor::desplazamiento(ed)));
    let pie = (acotar && hasta < total && editor::colocada(ed, hasta + 1)).then(|| editor::posicion(ed, hasta + 1).y);
    // Lo dibujado encima (ver `dibujar_rayas`) sigue valiendo si lo de después no se mueve y en lo pintado no hay nada
    // de lo que dibuja: así una letra en un .md grande no vuelve a preguntar dónde está cada cosa.
    let llanas = {
        let copia = a.copia.borrow();
        abierto_en(&copia, ini).is_none() && !md::hay_valla(&copia[ini..hasta]) && copia[ini..hasta].split(|&c| c == CR).all(|l| md::de_encima(l) == (false, false, 0))
    };
    editor::quieto(ed, || editor::oculto(ed, inicios.len() > OCULTO_DESDE, || {
        if let Some(p) = &pintado {
            // Con pocos tramos para lo largo que es el documento, la letra se pone sobre el texto que ya está: si hay
            // alguna lista numerada, RichEdit repasa el documento entero tras cada RTF que entra, y no al cambiar el
            // formato. Con las marcas a la vista el control no dibuja listas: el RTF no tiene ese coste. La raya va
            // siempre en RTF: puesta por tramos, a veces no se estira (prueba al azar, semilla 2).
            // Por tramos solo si el control ya lleva los saltos como quedan (blandos o no): la letra no los cambia.
            let a_tramos = ocultar
                && como_queda
                && p.tramos.len() <= TRAMOS_SIEMPRE + total / CARACTERES_POR_TRAMO
                && p.parrafos.iter().all(|x| !x.1.raya)
                && esperado.iter().all(|&c| letra_corriente(c));
            if a_tramos {
                for &(a, b, e) in &p.tramos {
                    editor::seleccionar(ed, ini + a, ini + b);
                    editor::letra(ed, &e, &colores);
                }
            } else {
                editor::seleccionar(ed, ini, fin);
                editor::meter_rtf(ed, &p.rtf);
                por_rtf = true;
            }
        }
        for &(x, y, e) in &bordes {
            editor::seleccionar(ed, x, y);
            editor::letra(ed, &e, &colores);
        }
        // El párrafo de cada línea, también de las vacías del final, que heredarían una viñeta. Cada una el suyo de una
        // vez: ponerlas todas normales y luego la viñeta cambiaba dos veces el párrafo de una lista en cada tecla, y
        // RichEdit vuelve a colocar el texto con cada cambio. Cada cambio cuesta casi medio milisegundo (15 s al
        // repintar entero un .md de 900 KB): las líneas seguidas con el mismo van juntas, y tras meter RTF, que deja con
        // el párrafo normal las que entran, solo se ponen los demás, el de la última (su salto no ha cambiado) y los de
        // las del final que no se pintan.
        let ultima = inicios.iter().rev().find(|&&l| l <= fin).copied().unwrap_or(ini);
        let fin_de = |k: usize| inicios.get(k + 1).map_or(hasta, |&x| x - 1);
        let mut grupo: Option<(usize, usize, md::Parrafo)> = None;
        let poner = |g: Option<(usize, usize, md::Parrafo)>| {
            if let Some((a, b, par)) = g {
                // Con el salto de la última: una selección que acaba donde empieza un párrafo (una línea vacía) lo deja
                // fuera.
                editor::seleccionar(ed, a, b + 1);
                editor::parrafo(ed, &par, ocultar);
            }
        };
        let mut distintos = pintado.iter().flat_map(|p| &p.parrafos).peekable();
        for (k, &l) in inicios.iter().enumerate() {
            let par = match distintos.peek() {
                Some(&&(pos, par)) if ini + pos == l => {
                    distintos.next();
                    par
                }
                _ => md::Parrafo::default(),
            };
            let par = hueco_de_imagen(l, par);
            if por_rtf && l < ultima && par == md::Parrafo::default() {
                poner(grupo.take());
            } else {
                match &mut grupo {
                    Some((_, b, p)) if *p == par && *b + 1 == l => *b = fin_de(k),
                    _ => {
                        poner(grupo.take());
                        grupo = Some((l, fin_de(k), par));
                    }
                }
            }
            if par.raya && imprimiendo() {
                let pos = l - ini;
                let largo = esperado[pos..].iter().position(|&c| c == CR).unwrap_or(esperado.len() - pos);
                editor::seleccionar(ed, l, l + largo);
                editor::raya(ed);
            }
        }
        poner(grupo.take());
    }));
    let quietas = pendiente.is_some_and(|(p, d)| editor::acotar(ed, p, d, ini, hasta, total, pie));
    // Por si el editor se pintó al volver a enseñarlo, con el formato a medias. Y si se ha dado formato a una raya, su
    // centro y su alto se vuelven a medir: los medidos antes pueden ser de cuando aún tenía el de otra cosa (código).
    a.rayas_hechas.set(encima.filter(|_| quietas && llanas));
    if pintado.iter().any(|p| p.parrafos.iter().any(|x| x.1.raya)) {
        a.forma_raya.set(None);
    }
    if editor::largo(ed) != total || editor::rango(ed, ini, fin) != proyeccion {
        fallo_vivo();
    }
}

/// [ini, fin) con los párrafos juntados que corta enteros (en el control, sus saltos blandos son md::JUNTO): el RTF le
/// pone su formato de párrafo desde el principio de lo que entra.
fn bloques_enteros(ed: HWND, t: &[u16], mut ini: usize, mut fin: usize) -> (usize, usize) {
    if editor::junta() {
        while ini > 0 && editor::rango(ed, ini - 1, ini) == [md::JUNTO] {
            ini = inicio_linea(t, ini - 1);
        }
        while fin < t.len() && editor::rango(ed, fin, fin + 1) == [md::JUNTO] {
            fin = fin_linea(t, fin + 1);
        }
    }
    (ini, fin)
}

/// Las rayas `---` de lo que se ve y, con las marcas ocultas, las de debajo de los títulos 1 y 2, el fondo de los bloques
/// de código y el fondo y la barra de las citas, encima de lo que ha pintado RichEdit (ver md::linea). Al imprimir no: las
/// rayas las lleva el texto.
pub fn dibujar_rayas(h: HWND) {
    let a = app();
    if !a.vivo.get() || imprimiendo() || a.vallas_en.try_borrow_mut().is_err() || a.copia.try_borrow().map_or(true, |c| c.len() != editor::largo(h)) {
        return;
    }
    let (z, texto, d) = (editor::zoom_actual(h), editor::formato(h), editor::desplazamiento(h));
    // RichEdit no repinta sus márgenes, y al desplazarse en horizontal (sin ajuste de línea) arrastra hasta ellos lo
    // dibujado encima: se limpian en cada pintado.
    editor::limpiar_margenes(h, &texto, tema::colores_md(en_oscuro()).fondo);
    let clave = {
        let cfg = a.cfg.borrow();
        (cfg.ajuste, cfg.marcas, cfg.margenes, texto.left, texto.right, z, cfg.tamano, cfg.letra.clone())
    };
    // Lo dibujado la última vez vale mientras no cambien el texto ni su formato (los dos lo borran), la forma de las
    // rayas ni lo que se ve: después de cada tecla RichEdit suele pintar más de una vez.
    let vista = (clave.clone(), d.x, d.y, texto.top, texto.bottom);
    if let Some((k, hechas)) = a.rayas_hechas.take()
        && k == vista
    {
        let repintado = editor::repintado().filter(|_| clave.0);
        dibujar_encima(h, &hechas, repintado);
        a.rayas_hechas.set(Some((k, hechas)));
        return;
    }
    let Ok(c) = a.copia.try_borrow() else { return };
    let forma = match a.forma_raya.take() {
        Some((k, f)) if k == clave => Some(f),
        _ => None,
    };
    // Las líneas que caben, con margen para las filas |---| plegadas, que son más bajas; sin saber el alto, de sobra.
    let n = forma.map_or(400, |(_, alto)| ((texto.bottom - texto.top) / alto.max(1)) as usize * 3 / 2 + 2);
    let (desde, hasta) = editor::lineas_visibles(h, n);
    // Desde el principio del párrafo juntado que asome por arriba: sus otras líneas no empiezan línea en el control.
    let l = bloques_enteros(h, &c, inicio_linea(&c, desde.min(c.len())), desde.min(c.len())).0;
    let Adornos { rayas, bajo, codigo, citas } = adornos(&c, l, hasta, clave.1);
    let total = c.len();
    drop(c);
    let px = |twips: i32| (twips * barras::dpi() * z + 72_000) / 144_000;
    let hechas = editor::rayas(h, &rayas, &bajo, total, (barras::dpi() * z / 9600).max(1), forma).and_then(|(forma, rayas)| {
        // Lo que va más abajo de lo que se ve no se pregunta (ver editor::lo_que_se_ve): su fondo llega al pie.
        let visto = std::cell::OnceCell::new();
        let fuera = |p: usize| p > *visto.get_or_init(|| editor::lo_que_se_ve(h).1);
        // Dónde acaba la línea que empieza en l y acaba en f: donde empieza la siguiente o, si es la última, `alto` más
        // abajo.
        let arriba = |l: usize| editor::colocada(h, l).then(|| editor::posicion(h, l).y);
        let pie = |l: usize, f: usize, alto: i32| match f < total {
            true if fuera(f + 1) => Some(texto.bottom),
            true => arriba(f + 1),
            false => arriba(l).map(|y| y + alto),
        };
        let mut e = Encima { codigo: Vec::new(), citas: Vec::new(), barras: Vec::new(), rayas };
        for &(l, cierre) in &codigo {
            if fuera(l) {
                break;
            }
            let abajo = match cierre {
                Some((l, f)) => pie(l, f, px(md::RELLENO))?,
                None => texto.bottom,
            };
            e.codigo.push(RECT { left: texto.left, top: arriba(l)?, right: texto.right, bottom: abajo });
        }
        // Con el ajuste de línea quitado, se mueven con el desplazamiento de lado.
        let x = |twips: i32| texto.left + px(twips) - d.x;
        for &(l, u, f, n) in &citas {
            if fuera(l) {
                break;
            }
            let (y, abajo) = (arriba(l)?, pie(u, f, px(md::EM) * 3 / 2)?);
            e.citas.push(RECT { left: x(md::MARGEN_CITA).max(texto.left), top: y, right: texto.right - px(md::MARGEN_CITA), bottom: abajo });
            for k in 0..n.min(8) as i32 {
                let izquierda = x(k * md::CITA + md::MARGEN_CITA);
                if izquierda >= texto.left {
                    e.barras.push(RECT { left: izquierda, top: y, right: izquierda + px(md::BARRA_CITA).max(1), bottom: abajo });
                }
            }
        }
        a.forma_raya.set(forma.map(|f| (clave, f)));
        Some(e)
    });
    let Some(hechas) = hechas else {
        // Se vuelve a pintar cuando RichEdit haya colocado las líneas; como mucho tres veces por cambio.
        if a.reintentos_raya.get() < 3 {
            a.reintentos_raya.set(a.reintentos_raya.get() + 1);
            unsafe { InvalidateRect(h, std::ptr::null(), 0) };
        }
        return;
    };
    let repintado = editor::repintado().filter(|_| vista.0.0);
    dibujar_encima(h, &hechas, repintado);
    a.rayas_hechas.set(Some((vista, hechas)));
}

/// Lo que va encima de las líneas desde la que empieza en `l` hasta la de `hasta`, en posiciones del texto.
pub struct Adornos {
    /// Las rayas `---` (dónde empieza y acaba su línea) y las de debajo de los títulos 1 y 2 (dónde acaba el título).
    pub rayas: Vec<(usize, usize)>,
    pub bajo: Vec<usize>,
    /// Los bloques de código: dónde empieza su primera línea y, si llega, su valla de cierre (dónde empieza y acaba).
    pub codigo: Vec<(usize, Option<(usize, usize)>)>,
    /// Las citas, en tramos seguidos del mismo nivel: dónde empiezan, dónde empieza y acaba su última línea y el nivel.
    pub citas: Vec<(usize, usize, usize, usize)>,
}

fn adornos(c: &[u16], mut l: usize, hasta: usize, marcas: bool) -> Adornos {
    let mut e = Adornos { rayas: Vec::new(), bajo: Vec::new(), codigo: Vec::new(), citas: Vec::new() };
    let mut abierto = abierto_apuntando(c, l, false);
    let mut bloque = abierto.map(|_| l);
    while l <= hasta.min(c.len()) {
        let f = fin_linea(c, l);
        let antes = abierto;
        if abierto.is_none() {
            let (regla, raya_debajo, n) = md::de_encima(&c[l..f]);
            if regla {
                e.rayas.push((l, f));
            } else if !marcas && f < c.len() && raya_debajo {
                e.bajo.push(f);
            }
            let n = if marcas { 0 } else { n };
            match e.citas.last_mut() {
                _ if n == 0 => {}
                Some(t) if t.2 + 1 == l && t.3 == n => (t.1, t.2) = (l, f),
                _ => e.citas.push((l, l, f, n)),
            }
        }
        abierto = md::estado_tras(&c[l..f], abierto);
        match (antes, abierto) {
            (None, Some(_)) => bloque = Some(l),
            (Some(_), None) => e.codigo.extend(bloque.take().map(|b| (b, Some((l, f))))),
            _ => {}
        }
        l = f + 1;
    }
    e.codigo.extend(bloque.map(|b| (b, None)));
    if marcas {
        e.codigo.clear();
    }
    e
}

/// Para imprimir (con `para_imprimir`), de las líneas desde la de `desde` hasta la de `hasta`: lo que va encima y las
/// tablas, cada una con dónde empieza cada fila y su diseño.
/// Cada tabla de la hoja: dónde empiezan sus filas y su diseño.
pub type TablasHoja = Vec<(Vec<usize>, Rc<tabla::Diseno>)>;

pub fn para_hoja(desde: usize, hasta: usize) -> (Adornos, TablasHoja) {
    let c = app().copia.borrow();
    let l = inicio_linea(&c, desde.min(c.len()));
    let encima = adornos(&c, l, hasta, false);
    let clave = clave_tablas();
    let tablas = tablas_de(&c, l, hasta)
        .into_iter()
        .filter_map(|(d, h)| {
            let dis = diseno(&c, d, h, clave)?;
            let mut filas = vec![d];
            while filas.len() < dis.altos.len() {
                filas.push(fin_linea(&c, filas[filas.len() - 1]) + 1);
            }
            Some((filas, dis))
        })
        .collect();
    (encima, tablas)
}

/// Lo de encima, solo donde ha repintado RichEdit (`repintado`; None, en toda la ventana): fuera de ahí sigue lo de la
/// vez anterior, y cambiar el fondo lee lo pintado de la pantalla, que cuesta. Sin ajuste de línea, en toda: al
/// desplazar de lado, RichEdit mueve lo pintado y el borde derecho de las citas no se mueve con ello.
fn dibujar_encima(h: HWND, e: &Encima, repintado: Option<RECT>) {
    let recortar = |zonas: &[RECT]| -> Vec<RECT> {
        let Some(r) = repintado else { return zonas.to_vec() };
        zonas
            .iter()
            .map(|z| RECT { left: z.left.max(r.left), top: z.top.max(r.top), right: z.right.min(r.right), bottom: z.bottom.min(r.bottom) })
            .filter(|z| z.left < z.right && z.top < z.bottom)
            .collect()
    };
    let c = tema::colores_md(en_oscuro());
    let fondos: Vec<(RECT, u32)> = recortar(&e.codigo).into_iter().map(|r| (r, c.fondo_codigo)).chain(recortar(&e.citas).into_iter().map(|r| (r, c.fondo_cita))).collect();
    editor::colorear_fondo(h, &fondos, c.fondo);
    editor::rellenar(h, &recortar(&e.barras), c.barra_cita);
    editor::rellenar(h, &recortar(&e.rayas), c.raya);
}

// --- Tablas dibujadas (ver tabla.rs) ---

/// Más filas no se dibujan: su diseño se rehace en cada cambio de la tabla.
const MAX_FILAS: usize = 500;
/// Un bloque de filas más largo no se trata como tabla (ver `tabla_en`).
const MAX_BLOQUE: usize = 5_000;
/// Volver a mirar las tablas cuando se suelte el botón del ratón (ver `mirar_tablas`).
const RELOJ_TABLAS: usize = 2;

/// Las tablas se dibujan con las marcas ocultas, y al imprimir.
fn con_tablas() -> bool {
    let a = app();
    a.vivo.get() && (imprimiendo() || !a.cfg.borrow().marcas)
}

/// El zoom, los puntos por pulgada y el ancho del texto (0 sin ajuste de línea): lo que cambia el diseño de una tabla.
fn clave_tablas() -> tabla::Clave {
    if let Some((ppp, ancho)) = IMPRIMIENDO.get() {
        return (100, ppp, ancho);
    }
    let a = app();
    let ed = a.ed.get();
    let r = editor::formato(ed);
    (editor::zoom_actual(ed), barras::dpi(), if a.cfg.borrow().ajuste { r.right - r.left } else { 0 })
}

/// Dónde empiezan las tablas de los dos extremos de la selección: esas se ven como texto, para editarlas.
fn abiertas_en(t: &[u16], (s, e): (usize, usize)) -> [Option<usize>; 2] {
    if imprimiendo() || app().vista.get() {
        return [None, None];
    }
    let de = |p: usize| tabla_en(t, inicio_linea(t, p.min(t.len()))).map(|x| x.0).filter(|&d| abierto_apuntando(t, d, false).is_none());
    [de(s), de(e)]
}

/// El diseño de la tabla t[desde..hasta]. None si no se dibuja: con muchísimas filas; con una que, plegada (a un
/// píxel por letra como poco), no cabe en una línea, que se partiría y saldría el doble de alta; o con una más alta que
/// el interlineado más alto que admite RichEdit (1.584 puntos, como Word).
fn diseno(t: &[u16], desde: usize, hasta: usize, clave: tabla::Clave) -> Option<Rc<tabla::Diseno>> {
    let mut d = app().disenos.borrow_mut();
    if d.0 != clave || d.1.len() > 256 {
        d.1.clear();
        d.0 = clave;
    }
    let texto = &t[desde..hasta];
    if let Some(x) = d.1.get(texto) {
        return x.clone();
    }
    let filas: Vec<&[u16]> = texto.split(|&c| c == CR).collect();
    let letra = (clave.0 * clave.1 + 11_999) / 12_000;
    // Cada celda se reserva aunque esté vacía: una fila de miles de barras sobre cientos de filas no cabría en memoria.
    let columnas = filas.iter().map(|f| f.iter().filter(|&&c| c == b'|' as u16).count()).max().unwrap_or(0);
    if filas.len() > MAX_FILAS || filas.len() * columnas > MAX_CELDAS || clave.2 > 0 && filas.iter().any(|f| f.len() as i32 * letra > clave.2 - 4) {
        return None;
    }
    let x = Some(Rc::new(tabla::disenar(&filas, clave.2, clave.0, clave.1)))
        .filter(|x| x.altos.iter().all(|&alto| tabla::twips(alto, clave.0, clave.1) <= 31_680));
    d.1.insert(texto.to_vec(), x.clone());
    x
}

/// Las tablas de las líneas t[ini..=fin] que no están dentro de un bloque de código: dónde empieza y acaba cada una.
fn tablas_de(t: &[u16], ini: usize, fin: usize) -> Vec<(usize, usize)> {
    let mut r = Vec::new();
    let mut abierto = abierto_apuntando(t, ini, false);
    let mut l = ini;
    while l <= fin.min(t.len()) {
        let f = fin_linea(t, l);
        if abierto.is_none() && md::fila(&t[l..f]) {
            if let Some((d, h, _)) = tabla_en(t, l) {
                r.push((d, h));
                l = h + 1;
                continue;
            }
            // Un bloque de filas que no es tabla: todas darían lo mismo.
            while l <= t.len() && md::fila(&t[l..fin_linea(t, l)]) {
                l = fin_linea(t, l) + 1;
            }
            continue;
        }
        abierto = md::estado_tras(&t[l..f], abierto);
        l = f + 1;
    }
    r
}

/// ¿Se puede dibujar la tabla t[x.0..x.1] por su número de filas?
fn pocas_filas(t: &[u16], x: &(usize, usize, usize)) -> bool {
    t[x.0..x.1].iter().filter(|&&c| c == CR).count() <= MAX_FILAS
}

/// Lo que hay que pintar de copia[ini..fin], ampliado a tablas enteras (el alto de cada fila depende de toda su tabla),
/// y las filas de las tablas que se dibujan, con su alto en twips.
fn plegadas(t: &[u16], ini: usize, fin: usize) -> (usize, usize, Vec<(usize, i32)>) {
    let a = app();
    let (abiertas, clave) = (abiertas_en(t, editor::seleccion(a.ed.get())), clave_tablas());
    // También las que tocan con lo pintado (una línea cambiada justo encima o debajo de una tabla la hace o la deshace),
    // enteras, pero solo las que se dibujan cerradas: las filas de una abierta se ven como texto, cada una por su lado; y
    // repintar entera una de miles de filas, que no se dibuja, costaría un segundo en cada tecla.
    let entera = |x: &(usize, usize, usize)| !abiertas.contains(&Some(x.0)) && pocas_filas(t, x);
    let ini = tabla_en(t, ini).filter(entera).map_or(ini, |x| ini.min(x.0));
    let fin = [inicio_linea(t, fin), (fin + 1).min(t.len())].into_iter().filter_map(|x| tabla_en(t, x)).filter(entera).fold(fin, |f, x| f.max(x.1));
    let tablas = tablas_de(t, ini, fin);
    let mut r = Vec::new();
    if tablas.is_empty() {
        return (ini, fin, r);
    }
    if a.clave_plegadas.get().is_none() {
        a.clave_plegadas.set(Some(clave));
    }
    let mut vistas = a.abiertas.borrow_mut();
    for (d, h) in tablas {
        vistas.retain(|&x| x != d);
        if abiertas.contains(&Some(d)) {
            vistas.push(d);
        } else if let Some(dis) = diseno(t, d, h, clave) {
            let mut p = d;
            for &alto in &dis.altos {
                r.push((p, tabla::twips(alto, clave.0, clave.1)));
                p = fin_linea(t, p) + 1;
            }
        }
    }
    (ini, fin, r)
}

/// Abre las tablas en que empieza o acaba la selección (se ven como texto, para editarlas) y vuelve a dibujar las que
/// deja. Con el botón del ratón pulsado no: al arrastrar, la tabla se abriría y cerraría bajo el puntero. Y si han
/// cambiado el zoom, el ancho o los puntos, vuelve a plegar todas a su nuevo alto.
pub fn mirar_tablas() {
    let a = app();
    a.tablas_avisadas.set(false);
    // Dentro de una acción de Bloc, al acabarla (ver `accion`).
    if !con_tablas() || a.aplicando.get() || a.en_accion.get() || editor::en_curso() {
        return;
    }
    // Con el botón pulsado (arrastrar una selección, el borde de la ventana), al soltarlo: el WM_LBUTTONUP puede no
    // llegar al editor.
    if pulsada(VK_LBUTTON) {
        unsafe { SetTimer(a.hwnd.get(), RELOJ_TABLAS, 100, None) };
        return;
    }
    let ed = a.ed.get();
    if editor::largo(ed) != a.copia.borrow().len() {
        return;
    }
    if a.clave_plegadas.get().is_some_and(|k| k != clave_tablas()) {
        replegar();
        return;
    }
    // Las que se ven abiertas y no tocan los extremos de la selección, y al revés.
    let tocadas: Vec<(usize, usize)> = {
        let t = a.copia.borrow();
        let ahora = abiertas_en(&t, editor::seleccion(ed));
        let mut vistas = a.abiertas.borrow_mut();
        let mut r: Vec<(usize, usize)> = Vec::new();
        let cambian: Vec<usize> = vistas.iter().filter(|d| !ahora.contains(&Some(**d))).chain(ahora.iter().flatten().filter(|d| !vistas.contains(d))).copied().collect();
        for d in cambian {
            match tabla_en(&t, inicio_linea(&t, d.min(t.len()))).filter(|x| x.0 == d) {
                Some(x) if !r.contains(&(x.0, x.1)) => r.push((x.0, x.1)),
                Some(_) => {}
                None => vistas.retain(|&x| x != d),
            }
        }
        r
    };
    let clave = clave_tablas();
    for (d, h) in tocadas {
        if diseno(&a.copia.borrow(), d, h, clave).is_some() {
            pintar(d, h);
            continue;
        }
        let ahora = abiertas_en(&a.copia.borrow(), editor::seleccion(ed));
        let mut vistas = a.abiertas.borrow_mut();
        vistas.retain(|&x| x != d);
        if ahora.contains(&Some(d)) {
            vistas.push(d);
        }
    }
}

/// Con otro zoom, ancho o puntos, las filas plegadas cambian de alto: a las tablas que ya estaban plegadas solo se les
/// pone el interlineado de cada fila, sin volver a dar formato a su texto (pintarlas todas costaba más de un segundo en
/// un .md de 900 KB con cientos de tablas). Las que pasan a dibujarse o a verse como texto, enteras.
fn replegar() {
    let a = app();
    let ed = a.ed.get();
    let clave = clave_tablas();
    // En la vista previa, solo las ya pintadas: las de detrás salen con su tramo (ver `tramo_vista`), y pintarlas aquí
    // una a una hacía a RichEdit recolocar todo lo de detrás en cada una (17 s en un .md de 256 KB).
    let todas = {
        let t = a.copia.borrow();
        let hasta = if a.vista.get() { VISTA_HASTA.with(Cell::get).min(t.len()) } else { t.len() };
        tablas_de(&t, 0, hasta)
    };
    let (filas, enteras) = editor::quieto(ed, || {
        let t = a.copia.borrow();
        let abiertas = a.abiertas.borrow();
        let (mut filas, mut enteras) = (Vec::new(), Vec::new());
        for (d, h) in todas.into_iter().filter(|x| !abiertas.contains(&x.0)) {
            // Plegada, sus letras son de un punto (ver md::linea).
            match (editor::de_un_punto(ed, d), diseno(&t, d, h, clave)) {
                (true, Some(dis)) => {
                    let mut p = d;
                    for &alto in &dis.altos {
                        filas.push((p, tabla::twips(alto, clave.0, clave.1)));
                        p = fin_linea(&t, p) + 1;
                    }
                }
                (false, None) => {}
                _ => enteras.push((d, h)),
            }
        }
        (filas, enteras)
    });
    editor::quieto(ed, || {
        editor::oculto(ed, filas.len() > OCULTO_DESDE, || {
            for &(p, alto) in &filas {
                editor::interlineado(ed, p, alto);
            }
        })
    });
    a.clave_plegadas.set(Some(clave));
    a.tablas_hechas.replace(None);
    a.rayas_hechas.set(None);
    for (d, h) in enteras {
        pintar(d, h);
    }
}

/// Los párrafos de los que ha salido la selección (`todos`, también los demás), con las marcas que les enseñó
/// `editor::seleccionar` otra vez escondidas: pintados de nuevo. Al acabar una acción van todos: el cursor de a medias
/// (tras sustituir, antes de poner la selección) los destapa aunque la selección final no lo necesite; si lo necesita,
/// al pintarlos se vuelven a destapar.
fn tapar(todos: bool) {
    let a = app();
    if !a.vivo.get() || a.aplicando.get() || a.en_accion.get() || editor::en_curso() {
        return;
    }
    let (s, e) = editor::seleccion(a.ed.get());
    let fuera: Vec<(usize, usize)> = editor::destapados(|d| {
        let (fuera, dentro) = d.iter().partition(|&&(x, y)| todos || s < x || e > y);
        *d = dentro;
        fuera
    });
    for (x, y) in fuera {
        pintar(x, y);
    }
}

/// Las tablas cerradas de lo que se ve, encima de lo que ha pintado RichEdit: sus filas están plegadas a su alto.
pub fn dibujar_tablas(h: HWND) {
    let a = app();
    if !con_tablas() || a.vallas_en.try_borrow_mut().is_err() || a.copia.try_borrow().map_or(true, |c| c.len() != editor::largo(h)) {
        return;
    }
    let clave = clave_tablas();
    if a.clave_plegadas.get().is_some_and(|k| k != clave) && !a.tablas_avisadas.replace(true) {
        unsafe { PostMessageW(a.hwnd.get(), WM_TABLAS, 0, 0) };
    }
    let (d, r, sel) = (editor::desplazamiento(h), editor::formato(h), editor::seleccion(h));
    let foco = unsafe { GetFocus() } == h;
    let vista = (clave, d.x, d.y, r.left, r.top, r.bottom, sel, foco, en_oscuro());
    let colores = tema::colores_md(en_oscuro());
    let dibujar = |hechas: &[Dibujada]| {
        if hechas.is_empty() {
            return;
        }
        unsafe {
            let dc = GetDC(h);
            let guardado = SaveDC(dc);
            IntersectClipRect(dc, r.left, r.top, r.right, r.bottom);
            for t in hechas {
                tabla::dibujar(dc, &t.diseno, t.x, t.y, &colores, t.sel, None);
            }
            RestoreDC(dc, guardado);
            ReleaseDC(h, dc);
        }
    };
    if let Some((k, hechas)) = &*a.tablas_hechas.borrow()
        && *k == vista
    {
        dibujar(hechas);
        return;
    }
    let Ok(c) = a.copia.try_borrow() else { return };
    // Las filas plegadas son más altas que una línea: con 400 se llega de sobra al final de lo que se ve.
    let (desde, hasta) = editor::lineas_visibles(h, 400);
    let abiertas = a.abiertas.borrow();
    let mut hechas = Vec::new();
    let mut colocadas = true;
    for (t0, t1) in tablas_de(&c, inicio_linea(&c, desde.min(c.len())), hasta.min(c.len())) {
        // Abierta, ni se diseña: su texto cambia en cada tecla.
        if abiertas.contains(&t0) {
            continue;
        }
        let Some(dis) = diseno(&c, t0, t1, clave) else { continue };
        if !editor::colocada(h, t0) {
            colocadas = false;
            continue;
        }
        let p = editor::posicion(h, t0);
        if p.y >= r.bottom {
            break;
        }
        hechas.push(Dibujada { desde: t0, x: p.x, y: p.y, diseno: dis, sel: foco && sel.0 < sel.1 && sel.0 <= t0 && sel.1 >= t1 });
    }
    drop((c, abiertas));
    a.tablas_vistas.set(!hechas.is_empty());
    dibujar(&hechas);
    // Si RichEdit aún no tenía alguna colocada, se vuelve a pintar cuando lo haya hecho, como con las rayas.
    if colocadas {
        a.tablas_hechas.replace(Some((vista, hechas)));
    } else if a.reintentos_raya.get() < 3 {
        a.reintentos_raya.set(a.reintentos_raya.get() + 1);
        unsafe { InvalidateRect(h, std::ptr::null(), 0) };
    }
}

// --- M6: las imágenes de un .md, debajo de su línea ---

/// Con las marcas ocultas y fuera de la vista previa (que solo tiene el flujo, no la carpeta) y de la impresión.
fn con_imagenes() -> bool {
    let a = app();
    a.vivo.get() && !a.cfg.borrow().marcas && !imprimiendo() && !a.vista.get()
}

/// La ruta de la imagen de una línea que es solo `![…](ruta)`: local, relativa a la carpeta del documento o completa.
/// Las de internet, nunca: se queda el texto. Tampoco las de otro equipo de la red (`\\equipo\...`) que no sea el del
/// documento: con solo abrir el .md, Windows le mandaría a ese equipo las credenciales de quien lo abre.
fn imagen_de_linea(l: &[u16]) -> Option<PathBuf> {
    if !md::solo_imagen(l) {
        return None;
    }
    ruta_de_imagen(&String::from_utf16_lossy(l), app().ruta.borrow().as_deref())
}

fn ruta_de_imagen(s: &str, doc: Option<&Path>) -> Option<PathBuf> {
    let (_, resto) = s.trim().strip_prefix("![")?.split_once("](")?;
    let ruta = resto.strip_suffix(')')?.split(" \"").next()?.trim();
    if ruta.is_empty() || ruta.contains(':') && !ruta.chars().nth(1).is_some_and(|c| c == ':') {
        return None;
    }
    let ruta = ruta.replace("%20", " ").replace("%28", "(").replace("%29", ")").replace("%25", "%").replace('/', "\\");
    let p = PathBuf::from(ruta);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico") {
        return None;
    }
    let p = if p.is_absolute() { p } else { doc?.parent()?.join(p) };
    let p = std::path::absolute(p).ok()?;
    let equipo = |p: &Path| match p.components().next() {
        Some(std::path::Component::Prefix(x)) => match x.kind() {
            std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_) => Some(None),
            std::path::Prefix::UNC(e, _) | std::path::Prefix::VerbatimUNC(e, _) => Some(Some(e.to_ascii_lowercase())),
            _ => None,
        },
        _ => None,
    };
    match equipo(&p)? {
        None => Some(p),
        Some(e) => (doc.and_then(equipo) == Some(Some(e))).then_some(p),
    }
}

/// Lo más grande que se ve una imagen, en píxeles: lo ancho del texto, contando siempre con la barra de desplazamiento
/// (si no, una imagen ancha la haría aparecer y desaparecer sin parar), y lo alto que deja el espacio después de un
/// párrafo de RichEdit (1.584 puntos), con el zoom.
fn caja_imagenes() -> (i32, i32) {
    let h = app().ed.get();
    let r = editor::formato(h);
    let barra = if unsafe { GetWindowLongW(h, GWL_STYLE) } as u32 & WS_VSCROLL != 0 { 0 } else { unsafe { GetSystemMetrics(SM_CXVSCROLL) } };
    let (z, ppp, _) = clave_tablas();
    ((r.right - r.left - px(4) - barra).max(px(16)), (31_680 * ppp as i64 * z.max(1) as i64 / (1440 * 100)) as i32 - px(8))
}

/// El tamaño con que se ve, en píxeles: el suyo, dentro de `caja_imagenes`. Nada mientras se carga (la primera vez se
/// manda cargar en otro hilo, como mucho CARGANDO_A_LA_VEZ) o si no se puede ver.
fn tamano_imagen(ruta: &Path) -> Option<(i32, i32)> {
    let a = app();
    let caja = caja_imagenes();
    let mut imagenes = a.imagenes.borrow_mut();
    match imagenes.get(ruta) {
        Some(Dibujo::Lista { ancho, alto, .. }) => Some(encajar((*ancho, *alto), caja)),
        Some(_) => None,
        None if imagenes.values().filter(|d| matches!(d, Dibujo::Cargando)).count() >= CARGANDO_A_LA_VEZ => None,
        None => {
            imagenes.insert(ruta.to_path_buf(), Dibujo::Cargando);
            let (v, r, fondo, vez) = (a.hwnd.get() as usize, ruta.to_path_buf(), tema::colores_md(en_oscuro()).fondo, a.vez_imagenes.get());
            // Reducida a lo que cabe en la pantalla: no se guarda entera una foto que se ve a lo ancho del texto.
            let pantalla = unsafe { (GetSystemMetrics(SM_CXVIRTUALSCREEN), 2 * GetSystemMetrics(SM_CYVIRTUALSCREEN)) };
            std::thread::spawn(move || {
                let cargada = crate::imagen::cargar(&r, fondo, pantalla);
                CARGADAS.lock().unwrap_or_else(|e| e.into_inner()).push((vez, r, cargada));
                unsafe { PostMessageW(v as HWND, WM_IMAGEN, 0, 1) };
            });
            None
        }
    }
}

/// (ancho, alto) a escala, sin pasar de `caja` en ninguno de los dos.
fn encajar((w, h): (i32, i32), (mw, mh): (i32, i32)) -> (i32, i32) {
    let (w, h) = (w.max(1) as i64, h.max(1) as i64);
    let (mw, mh) = (mw.max(1) as i64, mh.max(1) as i64);
    if w <= mw && h <= mh {
        return (w as i32, h as i32);
    }
    if w * mh >= h * mw { (mw as i32, (h * mw / w).max(1) as i32) } else { ((w * mh / h).max(1) as i32, mh as i32) }
}

/// Todas fuera, con sus mapas de bits: al abrir otro documento o cambiar el fondo.
fn soltar_imagenes() {
    let a = app();
    a.vez_imagenes.set(a.vez_imagenes.get().wrapping_add(1));
    for (_, d) in a.imagenes.borrow_mut().drain() {
        if let Dibujo::Lista { mapa, escalada, .. } = d {
            unsafe { DeleteObject(mapa as _) };
            if let Some((_, e)) = escalada {
                unsafe { DeleteObject(e as _) };
            }
        }
    }
}

/// El párrafo de una línea con una imagen ya cargada, con su hueco debajo (espacio después, en twips con el zoom).
fn hueco_de_imagen(l: usize, mut par: md::Parrafo) -> md::Parrafo {
    if par.codigo || !con_imagenes() {
        return par;
    }
    let ruta = {
        let c = app().copia.borrow();
        imagen_de_linea(&c[l.min(c.len())..fin_linea(&c, l.min(c.len()))])
    };
    if let Some((_, alto)) = ruta.as_deref().and_then(tamano_imagen) {
        let (z, ppp, _) = clave_tablas();
        par.despues = par.despues.max(((alto + px(8)) as i64 * 1440 * 100 / (ppp as i64 * z.max(1) as i64)) as i32);
    }
    par
}

/// Las líneas de imagen, colocadas otra vez: las de `solo` (las recién cargadas) o, sin `solo`, todas (otro zoom o
/// ancho). A las demás se les pregunta el tamaño, para que empiecen a cargar las que esperaban su turno.
fn colocar_imagenes(solo: Option<&std::collections::HashSet<PathBuf>>) {
    if !con_imagenes() {
        return;
    }
    let lineas: Vec<(usize, usize, PathBuf)> = {
        let c = app().copia.borrow();
        let mut v = Vec::new();
        let mut l = 0;
        while l < c.len() {
            let f = fin_linea(&c, l);
            if let Some(ruta) = imagen_de_linea(&c[l..f]) {
                v.push((l, f, ruta));
            }
            l = f + 1;
        }
        v
    };
    for (l, f, ruta) in lineas {
        if solo.is_none_or(|s| s.contains(&ruta)) {
            pintar(l, f);
        } else {
            tamano_imagen(&ruta);
        }
    }
}

/// Las imágenes de lo que se ve, en el hueco de debajo de su línea, encima de lo que ha pintado RichEdit. Solo donde
/// está el hueco: una línea de imagen dentro de un bloque de código, o juntada con otra, no lo tiene.
pub fn dibujar_imagenes(h: HWND) {
    let a = app();
    if !con_imagenes() {
        return;
    }
    let clave = clave_tablas();
    if a.clave_imagenes.replace(Some(clave)).is_some_and(|k| k != clave) {
        unsafe { PostMessageW(a.hwnd.get(), WM_IMAGEN, 0, 0) };
        return;
    }
    let Ok(c) = a.copia.try_borrow() else { return };
    if c.len() != editor::largo(h) {
        return;
    }
    let r = editor::formato(h);
    let (desde, hasta) = editor::lineas_visibles(h, 400);
    let mut l = inicio_linea(&c, desde.min(c.len()));
    let mut lineas = Vec::new();
    while l < hasta.min(c.len()) {
        let f = fin_linea(&c, l);
        if let Some(ruta) = imagen_de_linea(&c[l..f]) {
            lineas.push((l, f, ruta));
        }
        l = f + 1;
    }
    let n = c.len();
    drop(c);
    let mut sitios = Vec::new();
    for (l, f, ruta) in lineas {
        let Some((w, alto)) = tamano_imagen(&ruta) else { continue };
        let p = editor::posicion(h, l);
        // Lo de abajo del hueco: donde empieza la línea siguiente o, en la última, lo de abajo de la suya, que ya cuenta
        // el espacio después.
        let bajo = if f < n {
            if !editor::colocada(h, f + 1) {
                continue;
            }
            let sig = editor::posicion(h, f + 1).y;
            if sig - p.y < alto + px(16) {
                continue;
            }
            sig
        } else {
            let Some(y) = editor::bajo_de_linea(h, f) else { continue };
            y
        };
        if p.y > r.bottom {
            break;
        }
        sitios.push((ruta, p.x, bajo - alto - px(4), w, alto));
    }
    if sitios.is_empty() {
        return;
    }
    let mut imagenes = a.imagenes.borrow_mut();
    unsafe {
        let dc = GetDC(h);
        let guardado = SaveDC(dc);
        IntersectClipRect(dc, r.left, r.top, r.right, r.bottom);
        let mem = CreateCompatibleDC(dc);
        for (ruta, x, y, w, alto) in sitios {
            let Some(Dibujo::Lista { mapa, ancho, alto: alto0, escalada }) = imagenes.get_mut(&ruta) else { continue };
            // Escalada una vez por ancho: dibujarla no la vuelve a escalar en cada pintado.
            if escalada.is_none_or(|(e, _)| e != w) {
                if let Some((_, viejo)) = escalada.take() {
                    DeleteObject(viejo as _);
                }
                let nuevo = CreateCompatibleBitmap(dc, w, alto);
                let origen = CreateCompatibleDC(dc);
                let a0 = SelectObject(origen, *mapa as _);
                let a1 = SelectObject(mem, nuevo as _);
                SetStretchBltMode(mem, HALFTONE);
                StretchBlt(mem, 0, 0, w, alto, origen, 0, 0, *ancho, *alto0, SRCCOPY);
                SelectObject(mem, a1);
                SelectObject(origen, a0);
                DeleteDC(origen);
                *escalada = Some((w, nuevo as usize));
            }
            let Some((_, bmp)) = *escalada else { continue };
            let antes = SelectObject(mem, bmp as _);
            BitBlt(dc, x, y, w, alto, mem, 0, 0, SRCCOPY);
            SelectObject(mem, antes);
        }
        DeleteDC(mem);
        RestoreDC(dc, guardado);
        ReleaseDC(h, dc);
    }
}

/// Un clic en una tabla dibujada la abre, con el cursor al final del texto de esa celda. Devuelve si lo ha atendido.
pub fn clic_en_tabla(h: HWND, x: i32, y: i32) -> bool {
    let a = app();
    let destino = {
        let hechas = a.tablas_hechas.borrow();
        let d = editor::desplazamiento(h);
        let Some((_, hechas)) = hechas.as_ref().filter(|(k, _)| (k.1, k.2) == (d.x, d.y)) else { return false };
        let Some((desde, (fila, col))) = hechas.iter().find_map(|t| t.diseno.celda(x - t.x, y - t.y).map(|c| (t.desde, c))) else { return false };
        let c = a.copia.borrow();
        let l = (0..fila).fold(desde, |l, _| fin_linea(&c, l) + 1);
        let f = fin_linea(&c, l);
        let rayas: Vec<usize> = (l..f).filter(|&i| c[i] == b'|' as u16 && (i == l || c[i - 1] != b'\\' as u16)).collect();
        let Some(&x) = rayas.get(col) else { return false };
        let y = rayas.get(col + 1).copied().unwrap_or(f);
        (x + 1..y).rev().find(|&i| !md::espacio(c[i])).map_or((x + 2).min(y), |i| i + 1)
    };
    unsafe { SetFocus(h) };
    editor::seleccionar(h, destino, destino);
    mirar_tablas();
    true
}

/// Los márgenes del texto, según «Ver › Márgenes». Los normales, el 5 % del ancho en los Markdown y 6 píxeles en texto
/// plano. Arriba, siempre 6 píxeles: la primera línea no toca la barra.
fn margenes() {
    let a = app();
    let i = a.cfg.borrow().margenes as usize;
    let p = if a.vivo.get() && i == 1 {
        // Los de la vista previa del Explorador.
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { GetClientRect(a.ed.get(), &mut r) };
        r.right * 5 / 100
    } else {
        px(if a.vivo.get() { [8, 24, 72, 144] } else { [2, 6, 48, 120] }[i])
    };
    editor::margenes(a.ed.get(), p, px(6));
}

/// Ha entrado un md::JUNTO de verdad: el documento deja de juntar las líneas y el control vuelve a llevar sus saltos.
pub fn sin_juntar() {
    // Primero se repinta con los saltos de verdad, leyendo el control como el archivo (sus md::JUNTO como saltos); si no,
    // lo que entre después leería cada JUNTO como texto y el archivo perdería esos saltos.
    let a = app();
    a.desjuntando.set(true);
    pintar_todo();
    a.desjuntando.set(false);
    editor::juntar(false);
}

pub fn pintar_todo() {
    let a = app();
    a.forma_raya.set(None);
    a.clave_plegadas.set(None);
    a.abiertas.borrow_mut().clear();
    editor::destapados(|d| d.clear());
    let n = a.copia.borrow().len();
    if a.vista.get() {
        // La vista previa enseña antes lo que se ve, y el resto poco después (ver `tramo_vista`).
        let hasta = fin_linea(&a.copia.borrow(), TRAMO_VISTA.min(n));
        pintar(0, hasta);
        VISTA_HASTA.with(|v| v.set(hasta + 1));
        if hasta < n {
            unsafe { SetTimer(a.hwnd.get(), RELOJ_VISTA, ESPERA_VISTA, None) };
        }
        return;
    }
    pintar(0, n);
}

/// Lo que la vista previa pinta antes de enseñar un Markdown.
const TRAMO_VISTA: usize = 8 << 10;
const RELOJ_VISTA: usize = 9;
/// El resto, de una vez y cuando el archivo lleva un rato a la vista: con las flechas en el Explorador, el siguiente
/// llega antes y no se pinta para nada (256 KB cuestan ~1 s, y mientras el Explorador espera). En tramos costaba mucho
/// más: cada uno hace a RichEdit recolocar todo lo de detrás.
const ESPERA_VISTA: u32 = 400;
thread_local!(static VISTA_HASTA: Cell<usize> = const { Cell::new(0) });

fn tramo_vista() {
    let a = app();
    unsafe { KillTimer(a.hwnd.get(), RELOJ_VISTA) };
    let desde = VISTA_HASTA.with(Cell::get);
    let n = a.copia.borrow().len();
    if desde < n {
        pintar(desde, n);
        VISTA_HASTA.with(|v| v.set(n));
    }
}

/// Al dejar de enseñar un archivo: lo que quedaba por pintar ya no hace falta.
pub fn vista_parar() {
    unsafe { KillTimer(app().hwnd.get(), RELOJ_VISTA) };
}

// Si el control no ha quedado con el texto exacto (no debería pasar nunca), se vuelve a poner el texto de la copia,
// sin formato, y el Markdown sigue como texto plano.
fn fallo_vivo() {
    let a = app();
    let ed = a.ed.get();
    crate::w::apuntar("El formato en vivo ha cambiado el texto: se pasa a texto plano.");
    let t = a.copia.take();
    let (s, e) = editor::seleccion(ed);
    a.vivo.set(false);
    aplicando(|| {
        editor::deshacer_nativo(ed, true);
        aplicar_formato_base();
        editor::poner_texto(ed, &t);
        editor::seleccionar(ed, s, e);
    });
    a.historial.borrow_mut().limpiar();
    a.marca.set((0, 1, 0));
    poner_mensaje(crate::textos::t().sin_formato);
}

// --- Formato (barra y menú de los Markdown) ---

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Pone o quita negrita, cursiva, tachado o código a lo seleccionado, línea a línea, y deja seleccionado el mismo
/// texto. Si ya lo tiene todo, se quita. Sin selección, se quita a lo que tiene el cursor dentro o, si no lo tiene, se
/// ponen las dos marcas con el cursor en medio.
type LineaMd<'a> = (&'a [u16], md::Linea, usize, usize);

/// El bloque de código abierto al empezar la línea que empieza en `ini`. Se recuerda cada `PASO_VALLAS` caracteres: así
/// cada tecla o movimiento del cursor recorre como mucho eso, y no el texto desde el principio.
fn abierto_en(t: &[u16], ini: usize) -> Option<md::Valla> {
    abierto_apuntando(t, ini, true)
}

/// Como `abierto_en`, pero sin apuntar nada: para lo que se mira al pintar, que puede llegar entre un cambio del texto
/// y su `procesar`, con lo apuntado aún sin poner al día (romper una valla dejaba las líneas de debajo como código).
fn abierto_apuntando(t: &[u16], ini: usize, apuntar: bool) -> Option<md::Valla> {
    let a = app();
    if !a.vivo.get() {
        return md::estado_hasta(t, ini); // la copia no es la del control: sin recuerdo
    }
    if !a.hay_vallas.get() {
        return None;
    }
    let Ok(mut puntos) = a.vallas_en.try_borrow_mut() else { return md::estado_hasta(t, ini) };
    let k = puntos.partition_point(|&(q, _)| q <= ini);
    let (mut desde, mut antes) = if k > 0 { puntos[k - 1] } else { (0, None) };
    let mut nuevos = Vec::new();
    while ini - desde > PASO_VALLAS {
        let Some(d) = t[desde + PASO_VALLAS..ini].iter().position(|&c| c == CR) else { break };
        let corte = desde + PASO_VALLAS + d + 1;
        antes = md::estado_tras(&t[desde..corte - 1], antes);
        desde = corte;
        nuevos.push((desde, antes));
    }
    if apuntar {
        puntos.splice(k..k, nuevos);
    }
    if ini > desde { md::estado_tras(&t[desde..ini - 1], antes) } else { antes }
}

/// Las líneas que toca [s, e): cada una como la ve la vista y con lo seleccionado de ella.
fn lineas_md(t: &[u16], s: usize, e: usize) -> (usize, usize, Vec<LineaMd<'_>>) {
    let ini = inicio_linea(t, s);
    let fin = fin_linea(t, if e > s { e - 1 } else { e });
    let mut abierto = abierto_en(t, ini);
    let mut lineas = Vec::new();
    let mut p = ini;
    for l in t[ini..fin].split(|&c| c == CR) {
        let fin_l = p + l.len();
        lineas.push((l, md::Linea::leer(l, abierto), s.clamp(p, fin_l) - p, e.clamp(p, fin_l) - p));
        abierto = md::estado_tras(l, abierto);
        p = fin_l + 1;
    }
    (ini, fin, lineas)
}

/// ¿Ya tiene ese efecto lo seleccionado de estas líneas? None si ninguna lo dice (solo espacios, código...).
fn ya_tienen(lineas: &[LineaMd], efecto: u8) -> Option<bool> {
    // Con varias líneas, la que la selección no coge (una vacía entre dos párrafos) no cuenta.
    let varias = lineas.len() > 1;
    let mut votos = lineas.iter().filter(|x| !varias || x.2 < x.3).filter_map(|(l, ln, i, j)| ln.tiene(l, *i, *j, efecto)).peekable();
    votos.peek()?;
    Some(votos.all(|v| v))
}

fn envolver(efecto: u8, marca: &str) {
    let ed = app().ed.get();
    let m = u(marca);
    let t = texto();
    let (s, e) = editor::seleccion(ed);
    let e = e.min(t.len());
    let s = s.min(e);
    let (ini, fin, lineas) = lineas_md(&t, s, e);
    let vacia = s == e && s >= m.len() && t[s - m.len()..s] == m[..] && t[s..].starts_with(&m);
    if sin_letra(&lineas) && !(vacia && app().pareja.get() == Some((s, efecto))) {
        return;
    }
    if vacia {
        editor::seleccionar(ed, s - m.len(), s + m.len());
        editor::sustituir(ed, &[]);
        return;
    }
    let poner = ya_tienen(&lineas, efecto) != Some(true);
    if s == e && poner {
        // «~~~~» al principio de una línea abriría un bloque de código y «****» sola en ella sería una raya: ahí el
        // efecto va a la palabra de delante, y sin palabra no se pone nada. Decidido por Claude el 11/11, se puede cambiar.
        let (li, lf) = (inicio_linea(&t, s), fin_linea(&t, s));
        if md::valla_o_regla(&[&t[li..s], &m, &m, &t[s..lf]].concat()) && !md::valla_o_regla(&t[li..lf]) {
            let w = s + t[s..lf].iter().take_while(|&&c| char::from_u32(c as u32).is_some_and(char::is_alphanumeric)).count();
            if w > s {
                editor::seleccionar(ed, s, w);
                envolver(efecto, marca);
            }
            return;
        }
        editor::sustituir(ed, &[m.clone(), m.clone()].concat());
        editor::seleccionar(ed, s + m.len(), s + m.len());
        app().pareja.set(Some((s + m.len(), efecto)));
        return;
    }
    let mut nuevo = Vec::with_capacity(fin - ini + 8);
    let (mut ns, mut ne) = (s, e);
    for (k, (l, ln, i, j)) in lineas.iter().enumerate() {
        if k > 0 {
            nuevo.push(CR);
        }
        let q = ini + nuevo.len();
        // Sin nada seleccionado en ella, quitar el efecto quitaría la pareja que toca el borde de la selección.
        let (nl, ni, nj) = if lineas.len() > 1 && i == j { (l.to_vec(), *i, *j) } else { ln.alternar(l, *i, *j, efecto, poner) };
        if k == 0 {
            ns = q + ni;
        }
        if k + 1 == lineas.len() && e <= fin {
            ne = q + nj;
        }
        nuevo.extend(nl);
    }
    if nuevo[..] == t[ini..fin] {
        return;
    }
    if e > fin {
        ne = e - (fin - ini) + nuevo.len();
    }
    cambiar(ini, &t[ini..fin], &nuevo);
    editor::seleccionar(ed, ns, ne);
}

/// Pone `nuevo` en lugar de `viejo` (que empieza en `ini`), pero sustituyendo solo lo que cambia: lo sustituido coge
/// una sola letra, y como el texto de lo demás no cambia, nada lo volvería a pintar.
fn cambiar(ini: usize, viejo: &[u16], nuevo: &[u16]) {
    let ed = app().ed.get();
    if let Some((p, s)) = historial::diferencia(viejo, nuevo) {
        editor::seleccionar(ed, ini + p, ini + viejo.len() - s);
        editor::sustituir(ed, &nuevo[p..nuevo.len() - s]);
    }
}

/// En un bloque de código o en la fila |---| de una tabla no hay formato de letra: los botones no hacen nada.
fn sin_letra(lineas: &[LineaMd]) -> bool {
    !lineas.iter().any(|x| x.1.con_letra())
}

/// Un enlace que coge una línea de código cambia el código (o deja abierta su valla), y uno que coge una raya de tabla
/// parte la fila.
fn cabe_enlace(t: &[u16], ini: usize, fin: usize) -> bool {
    let lineas = lineas_md(t, ini, fin).2;
    !(lineas.iter().any(|x| !x.1.con_letra()) || lineas.iter().any(|x| md::fila(x.0)) && t[ini..fin].contains(&(b'|' as u16)))
}

fn es_direccion(s: &[u16]) -> bool {
    let empieza = |p: &str| s.len() >= p.len() && s.iter().zip(p.encode_utf16()).all(|(a, b)| *a == b);
    empieza("http://") || empieza("https://")
}

/// Un bloque pegado (un bloque de código, una tabla), con una línea en blanco delante y detrás si no la hay, y en su
/// propia línea si se pega en medio de una.
pub fn en_sus_lineas(bloque: &[u16]) -> Vec<u16> {
    let t = texto();
    let (s, e) = editor::seleccion(app().ed.get());
    let e = e.min(t.len());
    let (antes, despues) = (&t[..s.min(e)], &t[e..]);
    let salto = |c: &u16| *c == CR || *c == md::JUNTO;
    let mut r = Vec::with_capacity(bloque.len() + 4);
    match antes.split_last() {
        Some((c, _)) if !salto(c) => r.extend([CR, CR]),
        Some((_, previo)) if previo.rsplit(salto).next().is_some_and(|l| !blanca(l)) => r.push(CR),
        _ => {}
    }
    r.extend_from_slice(bloque);
    match despues.split_first() {
        Some((c, _)) if !salto(c) => r.extend([CR, CR]),
        Some((_, resto)) if resto.split(salto).next().is_some_and(|l| !blanca(l)) => r.push(CR),
        _ => {}
    }
    r
}

/// Pegar una dirección sobre un trozo de una línea de un Markdown lo convierte en su enlace, `[trozo](dirección)`.
pub fn enlace_al_pegar(pegado: &[u16]) -> Option<Vec<u16>> {
    let a = app();
    let espacio = |c: &u16| matches!(*c, 9 | 10 | 13 | 32);
    let ini = pegado.iter().position(|c| !espacio(c))?;
    let fin = pegado.iter().rposition(|c| !espacio(c))? + 1;
    let url = &pegado[ini..fin];
    if !a.md.get() || solo_ver() || !es_direccion(url) || url.iter().any(espacio) {
        return None;
    }
    let (s, e) = editor::seleccion(a.ed.get());
    if s >= e {
        return None;
    }
    let t = texto();
    let e = e.min(t.len());
    let s = s.min(e);
    let sel = &t[s..e];
    let recortada = sel.iter().position(|c| !espacio(c)).map_or(&sel[..0], |k| &sel[k..]);
    // El texto de un enlace que ya lo es («[texto](vieja)»): un enlace dentro de otro no.
    let ya_enlace = s > 0 && t[s - 1] == b'[' as u16 && t[e..].starts_with(&[b']' as u16, b'(' as u16]);
    if sel.is_empty() || sel.iter().any(|&c| c == CR || c == md::JUNTO) || es_direccion(recortada) || ya_enlace || !cabe_enlace(&t, s, e) {
        return None;
    }
    Some([u("["), sel.to_vec(), u("]("), url.to_vec(), u(")")].concat())
}

fn enlace() {
    let ed = app().ed.get();
    let t = texto();
    let (ini, fin) = editor::seleccion(ed);
    let fin = fin.min(t.len());
    let ini = ini.min(fin);
    if !cabe_enlace(&t, ini, fin) {
        return;
    }
    let sel = t[ini..fin].to_vec();
    let url = es_direccion(&sel);
    let nuevo = if url { [u("[]("), sel, u(")")].concat() } else { [u("["), sel, u("]()")].concat() };
    editor::sustituir(ed, &nuevo);
    let p = if url { ini + 1 } else { ini + nuevo.len() - 1 };
    editor::seleccionar(ed, p, p);
}

/// Inicio va primero a la primera letra de la línea, tras la sangría (y en un .md tras la cita y la marca de lista), y
/// otra vez al principio de la línea, como en los editores de código. Solo en la primera línea a la vista de un
/// párrafo: en las demás y en una línea sin sangría ni marca, el Inicio de RichEdit. Con Mayús, alarga la selección.
pub fn inicio(mayus: bool) -> bool {
    let a = app();
    let ed = a.ed.get();
    let (ancla, activo) = editor::seleccion_con_sentido(ed);
    let desde = editor::inicio_a_la_vista(ed, activo);
    if desde > 0 && editor::rango(ed, desde - 1, desde) != [CR] {
        return false;
    }
    let l = editor::rango(ed, desde, (desde + 1024).min(editor::largo(ed)));
    let l = &l[..l.iter().position(|&c| c == CR).unwrap_or(l.len())];
    let espacios = |l: &[u16]| l.iter().take_while(|&&c| c == b' ' as u16 || c == 9).count();
    let n = if a.md.get() {
        let q = largo_cita(l);
        md::marca_lista(&l[q..]).map_or_else(|| q + espacios(&l[q..]), |(_, fin)| q + fin)
    } else {
        espacios(l)
    };
    if n == 0 {
        return false;
    }
    let destino = if activo == desde + n { desde } else { desde + n };
    if mayus {
        editor::seleccionar_con_sentido(ed, ancla, destino);
    } else {
        editor::seleccionar(ed, destino, destino);
    }
    true
}

/// Intro en una línea sangrada: la nueva empieza con la misma sangría, la de espacios y tabuladores de antes del cursor.
fn intro_sangrado(ed: HWND, s: usize) -> bool {
    let antes = editor::rango(ed, s.saturating_sub(4096), s);
    // Sin salto en lo leído, el principio de lo leído no es el de la línea.
    let Some(l) = antes.iter().rposition(|&c| c == CR).map(|i| &antes[i + 1..]).or((s <= 4096).then_some(&antes[..])) else { return false };
    let n = l.iter().take_while(|&&c| c == b' ' as u16 || c == 9).count();
    if n == 0 {
        return false;
    }
    editor::sustituir(ed, &[&[CR][..], &l[..n]].concat());
    true
}

/// Las marcas de cita del principio de una línea, con su sangría y su espacio: «> » o «> > ».
fn largo_cita(l: &[u16]) -> usize {
    let mut k = 0;
    loop {
        let s = k + l[k..].iter().take_while(|&&c| c == b' ' as u16).count();
        if s - k > 3 || l.get(s) != Some(&(b'>' as u16)) {
            return k;
        }
        k = s + 1;
        if l.get(k) == Some(&(b' ' as u16)) {
            k += 1;
        }
    }
}

/// Dónde empieza el texto de una línea: tras la sangría, las citas y la marca de título o de lista, con los espacios
/// de más que la sigan (los botones los quitan al cambiar la marca).
fn largo_prefijo(l: &[u16]) -> usize {
    let q = largo_cita(l);
    let r = &l[q..];
    let m = md::prefijo_titulo(r).map(|(_, m)| m).or_else(|| md::marca_lista(r).map(|(_, m)| m)).unwrap_or(0);
    q + m + sangria_de(&r[m..])
}

/// Cambia el principio de las líneas enteras que toca la selección (su marca de título, lista o cita) y deja
/// seleccionado el mismo texto. En los Markdown, `cambio` recibe las líneas que admiten marca, y de cada una si sigue
/// el párrafo de la de encima tras un salto blando (ver md::salto_blando): las de un bloque de código y las filas de una
/// tabla se quedan como están (un título o una lista las rompería). Un párrafo juntado cambia entero y, con `unir` (un
/// título, que es siempre una línea), sus líneas pasan a ser una.
fn prefijar(unir: bool, cambio: impl FnOnce(Vec<Vec<u16>>, Vec<bool>) -> Vec<Vec<u16>>) {
    let ed = app().ed.get();
    let en_md = app().md.get();
    let original = texto();
    let (a, b) = editor::seleccion(ed);
    let b = b.min(original.len());
    let a = a.min(b);
    let (ini, fin) = (inicio_linea(&original, a), fin_linea(&original, if b > a { b - 1 } else { a }));
    let (ini, fin) = bloques_enteros(ed, &original, ini, fin);
    let control = if editor::junta() { editor::rango(ed, ini, fin) } else { Vec::new() };
    let mut blandos: Vec<usize> = control.iter().enumerate().filter(|x| *x.1 == md::JUNTO).map(|x| ini + x.0).collect();
    let unido;
    let (t, a, b, fin, quitado): (&[u16], usize, usize, usize, usize) = if unir && !blandos.is_empty() {
        let (u, mover) = unir_lineas(&original, &blandos);
        let quitado = original.len() - u.len();
        blandos.clear();
        unido = u;
        (&unido, mover(a), mover(b), fin - quitado, quitado)
    } else {
        (&original, a, b, fin, 0)
    };
    let viejas: Vec<Vec<u16>> = t[ini..fin].split(|&c| c == CR).map(<[u16]>::to_vec).collect();
    let fijas: Vec<bool> = if en_md {
        let mut abierto = abierto_en(t, ini);
        viejas
            .iter()
            .map(|l| {
                let fija = !md::Linea::leer(l, abierto).con_letra() || md::fila(l);
                abierto = md::estado_tras(l, abierto);
                fija
            })
            .collect()
    } else {
        vec![false; viejas.len()]
    };
    let mut sigue = Vec::with_capacity(viejas.len());
    let mut p = ini;
    for v in &viejas {
        sigue.push(p > 0 && blandos.binary_search(&(p - 1)).is_ok());
        p += v.len() + 1;
    }
    let prefijo = if en_md { largo_prefijo } else { sangria_de };
    let libres = |x: &[bool]| x.iter().zip(&fijas).filter(|y| !y.1).map(|y| *y.0).collect::<Vec<bool>>();
    let mut cambiadas = cambio(viejas.iter().zip(&fijas).filter(|x| !x.1).map(|x| x.0.clone()).collect(), libres(&sigue)).into_iter();
    let nuevas: Vec<Vec<u16>> = viejas.iter().zip(&fijas).map(|(v, &f)| if f { v.clone() } else { cambiadas.next().unwrap_or_else(|| v.clone()) }).collect();
    let movidas: Vec<usize> = (0..nuevas.len()).filter(|&i| nuevas[i] != viejas[i]).collect();
    let sin_numerar = !en_md || fijas.iter().any(|&f| f) || abierto_en(t, ini).is_some();
    let (nuevas, cola, hasta) = if sin_numerar || movidas.is_empty() { (nuevas, Vec::new(), fin) } else { renumerar(t, ini, fin, nuevas, &movidas) };
    let nuevo = [nuevas.join(&CR), cola].concat();
    // Lo que va detrás de la marca no se mueve dentro de su línea: va con el final que tienen en común la línea vieja y
    // la nueva (el «- » del texto de un título pasa a ser la marca de la lista). Lo de delante pasa al principio del texto.
    let mover = |p: usize| {
        if p > fin {
            return p + nuevo.len() - (hasta - ini);
        }
        let (mut v0, mut n0) = (ini, ini);
        for (v, n) in viejas.iter().zip(&nuevas) {
            if p <= v0 + v.len() {
                if v == n {
                    return n0 + p - v0;
                }
                let comun = v.iter().rev().zip(n.iter().rev()).take_while(|(x, y)| x == y).count();
                let (detras, mn) = (v0 + v.len() - p, prefijo(n));
                let q = if detras <= comun { (n.len() - detras).max(mn) } else { mn };
                return n0 + q.min(n.len());
            }
            v0 += v.len() + 1;
            n0 += n.len() + 1;
        }
        p
    };
    let (na, nb) = (mover(a), mover(b));
    cambiar(ini, &original[ini..hasta + quitado], &nuevo);
    editor::seleccionar(ed, na, nb);
}

/// Lo que no se ve al principio de una línea que sigue un párrafo juntado: su marca de cita y su sangría.
fn escondido(l: &[u16]) -> usize {
    let q = largo_cita(l);
    q + sangria_de(&l[q..])
}

/// t con los saltos blandos de `blandos` (de menor a mayor) cambiados por un espacio y sin lo que lleva delante la
/// línea que sigue (su marca de cita y su sangría), y adónde va a parar en él cada posición de t.
fn unir_lineas(t: &[u16], blandos: &[usize]) -> (Vec<u16>, impl Fn(usize) -> usize + use<>) {
    let quitas: Vec<(usize, usize)> = blandos.iter().map(|&k| (k + 1, escondido(&t[k + 1..fin_linea(t, k + 1)]))).collect();
    let mut r = Vec::with_capacity(t.len());
    let mut desde = 0;
    for &(i, q) in &quitas {
        r.extend_from_slice(&t[desde..i - 1]);
        r.push(b' ' as u16);
        desde = i + q;
    }
    r.extend_from_slice(&t[desde..]);
    (r, move |p: usize| p - quitas.iter().filter(|&&(i, _)| i < p).map(|&(i, q)| (p - i).min(q)).sum::<usize>())
}

/// Las líneas `nuevas`, que van en lugar de las de `t[ini..fin]`, con las listas numeradas vueltas a numerar (ver
/// md::renumerar; `movidas`, las que han cambiado). Devuelve esas líneas, lo que sigue a `fin` ya numerado (con su
/// salto delante) y hasta dónde llega eso en `t`.
fn renumerar(t: &[u16], ini: usize, fin: usize, nuevas: Vec<Vec<u16>>, movidas: &[usize]) -> (Vec<Vec<u16>>, Vec<u16>, usize) {
    const LINEAS: usize = 1000;
    let mut cabeza: Vec<Vec<u16>> = t[..ini.saturating_sub(1)].rsplit(|&c| c == CR).take_while(|l| md::sigue_lista(l)).take(LINEAS).map(<[u16]>::to_vec).collect();
    if ini == 0 {
        cabeza.clear();
    }
    cabeza.reverse();
    let cola: Vec<&[u16]> = if fin < t.len() { t[fin + 1..].split(|&c| c == CR).take_while(|l| md::sigue_lista(l)).take(LINEAS).collect() } else { Vec::new() };
    let hasta = fin + cola.iter().map(|l| l.len() + 1).sum::<usize>();
    let (k, n) = (cabeza.len(), nuevas.len());
    let mut lineas = cabeza;
    lineas.extend(nuevas);
    lineas.extend(cola.iter().map(|l| l.to_vec()));
    md::renumerar(&mut lineas, k, &movidas.iter().map(|i| i + k).collect::<Vec<_>>());
    let cola: Vec<u16> = lineas[k + n..].iter().flat_map(|l| std::iter::once(CR).chain(l.iter().copied())).collect();
    (lineas[k..k + n].to_vec(), cola, hasta)
}

/// ¿Son ya todas títulos de ese nivel, sin contar las vacías (y dentro de su cita, si la tienen)?
fn todas_de_nivel<L: AsRef<[u16]>>(ls: &[L], nivel: usize) -> bool {
    let de_nivel = |l: &[u16]| md::prefijo_titulo(&l[largo_cita(l)..]).is_some_and(|(n, _)| n == nivel);
    ls.iter().any(|l| !blanca(l.as_ref())) && ls.iter().all(|l| blanca(l.as_ref()) || de_nivel(l.as_ref()))
}

/// H1, H2, H3: si todas las líneas ya son títulos de ese nivel, dejan de serlo; si no, todas pasan a serlo. Un título
/// va dentro de su cita y en lugar de la marca de lista: «## - texto» se vería con el guion.
fn encabezar(nivel: usize) {
    if partir_para_titulo(nivel) {
        return;
    }
    prefijar(true, |ls, _| {
        let todas = todas_de_nivel(&ls, nivel);
        let varias = ls.len() > 1;
        ls.into_iter()
            .map(|l| {
                if blanca(&l) && varias {
                    return l;
                }
                let q = largo_cita(&l);
                let r = &l[q..];
                let resto = match md::prefijo_titulo(r) {
                    Some((_, m)) => &r[m..],
                    None => md::marca_lista(r).map_or(r, |(_, m)| &r[m..]),
                };
                let resto = &resto[sangria_de(resto)..];
                let marca = if todas { Vec::new() } else { [vec![b'#' as u16; nivel], vec![b' ' as u16]].concat() };
                [&l[..q], &marca, resto].concat()
            })
            .collect()
    });
}

/// Un título es siempre una línea entera. Con parte de una línea seleccionada, lo seleccionado pasa a su propia línea
/// como título, y lo de antes y lo de después se quedan cada uno en la suya (en la cita, si la había). Decidido por
/// Claude el 09/11, se puede cambiar. Devuelve si lo ha hecho.
fn partir_para_titulo(nivel: usize) -> bool {
    let ed = app().ed.get();
    let t = texto();
    let (a, b) = editor::seleccion(ed);
    let b = b.min(t.len());
    let a = a.min(b);
    let (ini, fin, lineas) = lineas_md(&t, a, b);
    let l = &t[ini..fin];
    let q = largo_cita(l);
    if a == b || lineas.len() > 1 || sin_letra(&lineas) || md::fila(l) || md::prefijo_titulo(&l[q..]).is_some() {
        return false;
    }
    let p = largo_prefijo(l);
    // Lo seleccionado de la línea, sin su salto ni lo que caiga en la marca.
    let (mut x, mut y) = ((a - ini).max(p), (b - ini).min(l.len()));
    if x >= y {
        return false;
    }
    while x < y && blanca(&l[x..x + 1]) {
        x += 1;
    }
    while y > x && blanca(&l[y - 1..y]) {
        y -= 1;
    }
    let (hay_antes, hay_despues) = (!blanca(&l[p..x]), !blanca(&l[y..]));
    if x == y || !hay_antes && !hay_despues {
        return false;
    }
    let mut nuevo = Vec::with_capacity(l.len() + 2 * q + nivel + 3);
    if hay_antes {
        let antes = &l[..x];
        nuevo.extend_from_slice(&antes[..antes.len() - antes.iter().rev().take_while(|&&c| c == b' ' as u16 || c == 9).count()]);
        nuevo.push(CR);
    }
    nuevo.extend_from_slice(&l[..q]);
    nuevo.extend(std::iter::repeat_n(b'#' as u16, nivel));
    nuevo.push(b' ' as u16);
    let sel = ini + nuevo.len();
    nuevo.extend_from_slice(&l[x..y]);
    let sel_fin = ini + nuevo.len();
    if hay_despues {
        nuevo.push(CR);
        nuevo.extend_from_slice(&l[..q]);
        nuevo.extend_from_slice(&l[y + sangria_de(&l[y..])..]);
    }
    cambiar(ini, l, &nuevo);
    editor::seleccionar(ed, sel, sel_fin);
    true
}

/// 0 viñetas, 1 numerada, 2 casillas, 3 cita. Si todas las líneas ya la tienen, se quita.
/// Tab con algo seleccionado y Mayús+Tab: meten o sacan las líneas, todas lo mismo. En los Markdown, un elemento de
/// lista entra en el de encima (tanto como su marca: así lo lee GitHub) y sale a la altura de la línea de encima que esté
/// más fuera; lo demás, 4 espacios. En texto plano, un tabulador.
/// Si el cursor está en un elemento de lista de un Markdown, fuera de un bloque de código.
pub fn en_lista() -> bool {
    let a = app();
    if !a.md.get() {
        return false;
    }
    let t = texto();
    let ini = inicio_linea(&t, editor::seleccion(a.ed.get()).0.min(t.len()));
    let ini = bloques_enteros(a.ed.get(), &t, ini, ini).0;
    md::marca_lista(&t[ini..fin_linea(&t, ini)]).is_some() && abierto_en(&t, ini).is_none()
}

pub fn sangrar(mas: bool) {
    let a = app();
    let en_md = a.md.get();
    let t = texto();
    let ini = inicio_linea(&t, editor::seleccion(a.ed.get()).0);
    let ini = bloques_enteros(a.ed.get(), &t, ini, ini).0;
    let linea = &t[ini..fin_linea(&t, ini)];
    let espacios = |l: &[u16]| l.iter().take_while(|&&c| c == b' ' as u16).count();
    // El ancho de la sangría, con el tabulador hasta la columna siguiente múltiplo de 4, como lo cuenta Markdown.
    let ancho = |l: &[u16]| l.iter().take_while(|&&c| c == b' ' as u16 || c == 9).fold(0, |k, &c| if c == 9 { k / 4 * 4 + 4 } else { k + 1 });
    let n = ancho(linea);
    // Las líneas con algo escrito de encima, de la más cercana hacia arriba.
    let encima = || t[..ini.saturating_sub(1)].rsplit(|&c| c == CR).filter(|l| !blanca(l)).take(500);
    let mut tab = !en_md;
    let paso = if !en_md {
        0
    } else if mas {
        let hermana = encima().take_while(|l| ancho(l) >= n).find(|l| ancho(l) == n);
        // Un elemento de lista baja como mucho un nivel por debajo del de encima: necesita uno del mismo nivel del que
        // colgar (como Word y Obsidian).
        if md::marca_lista(linea).is_some() && hermana.and_then(md::marca_lista).is_none() {
            return;
        }
        // Las sublistas, con tabuladores como Obsidian, salvo que la lista ya vaya con espacios: así cada nivel es un
        // tabulador y se ve en su nivel (con espacios, «1.» pide 3 y Bloc cuenta un nivel cada 2).
        let sangrada = encima().take_while(|l| md::sigue_lista(l)).find(|l| ancho(l) > 0);
        tab = md::marca_lista(linea).is_some() && sangrada.is_none_or(|l| l[0] == 9);
        hermana.and_then(|l| md::marca_lista(l).map(|(i, m)| if md::es_de_tipo(l, 2) { 2 } else { m - i })).unwrap_or(4)
    } else {
        espacios(linea) - encima().map(espacios).find(|&k| k < espacios(linea)).unwrap_or(espacios(linea).saturating_sub(4))
    };
    prefijar(false, |ls, _| {
        let varias = ls.len() > 1;
        ls.into_iter()
            .map(|l| {
                // Un título o una cita con cuatro espacios delante dejan de serlo.
                if en_md && (md::prefijo_titulo(&l).is_some() || largo_cita(&l) > 0) {
                    l
                } else if mas {
                    if blanca(&l) && varias {
                        l
                    } else if tab {
                        [vec![9], l].concat()
                    } else {
                        [vec![b' ' as u16; paso], l].concat()
                    }
                } else {
                    let k = if l.first() == Some(&9) { 1 } else { espacios(&l).min(if en_md { paso } else { 4 }) };
                    l[k..].to_vec()
                }
            })
            .collect()
    });
}

/// Retroceso justo detrás de una marca de lista o de cita que no se ve (la viñeta o la barra las dibuja el control): se
/// va la marca entera, como en Word, y no solo su espacio, que dejaría «-texto». Devuelve si lo ha hecho.
pub fn retroceso() -> bool {
    let a = app();
    if !a.vivo.get() || a.cfg.borrow().marcas {
        return false;
    }
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let t = a.copia.borrow();
    if s != e || s > t.len() {
        return false;
    }
    // Al principio de lo que se ve de una línea que sigue un párrafo juntado, tras su espacio: se va el salto blando
    // con lo escondido de la línea, como al borrar un espacio. Si no, se iría su marca de cita y saldría de la cita.
    let ini = inicio_linea(&t, s);
    if ini > 0 && s <= ini + escondido(&t[ini..fin_linea(&t, ini)]) && editor::rango(ed, ini - 1, ini) == [md::JUNTO] {
        let hasta = ini + escondido(&t[ini..fin_linea(&t, ini)]);
        drop(t);
        editor::seleccionar(ed, ini - 1, hasta);
        editor::sustituir(ed, &[]);
        return true;
    }
    let (ini, _, lineas) = lineas_md(&t, s, s);
    let (l, linea, _, _) = &lineas[0];
    let q = largo_cita(l);
    let marca = if !linea.con_letra() {
        None
    } else if let Some((i, m)) = md::marca_lista(&l[q..]) {
        Some((q + i, q + m, true)) // también la de una lista dentro de una cita
    } else if q > 0 {
        Some((sangria_de(l), q, false)) // las de una cita anidada, todas: se esconden todas
    } else {
        None
    };
    // Con los espacios de más que lleve detrás: también se esconden («-   texto»).
    let marca = marca.map(|(i, m, lista)| (i, m + l[m..].iter().take_while(|&&c| c == b' ' as u16).count(), lista));
    // Delante de la marca de una lista o de una cita (al principio de la línea), RichEdit quitaría la viñeta o la barra
    // solo de la pantalla, como Word: también se quita la marca.
    let Some((i, m, lista)) = marca.filter(|&(_, m, _)| s <= ini + m) else { return false };
    drop(t);
    // En un subnivel, sube un nivel (como Word); en el primero, se va la viñeta.
    if lista && q == 0 && i > 0 {
        sangrar(false);
        return true;
    }
    editor::seleccionar(ed, ini + i, ini + m);
    editor::sustituir(ed, &[]);
    true
}

/// Supr justo delante de un salto blando: se va con lo escondido de la línea de después, como con Retroceso. Devuelve
/// si lo ha hecho.
pub fn suprimir() -> bool {
    let a = app();
    if !a.vivo.get() || a.cfg.borrow().marcas {
        return false;
    }
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let t = a.copia.borrow();
    if s != e || s >= t.len() || t[s] != CR || editor::rango(ed, s, s + 1) != [md::JUNTO] {
        return false;
    }
    let hasta = s + 1 + escondido(&t[s + 1..fin_linea(&t, s + 1)]);
    drop(t);
    editor::seleccionar(ed, s, hasta);
    editor::sustituir(ed, &[]);
    true
}

fn sangria_de(l: &[u16]) -> usize {
    l.iter().take_while(|&&c| c == b' ' as u16 || c == 9).count()
}

fn blanca(l: &[u16]) -> bool {
    l.iter().all(|&c| c == b' ' as u16 || c == 9)
}

/// ¿Son ya todas de ese tipo, sin contar las vacías? Con alguna que no lo sea: en una línea vacía, el botón pone la
/// marca. Una lista dentro de una cita cuenta como lista (y como cita).
fn todas_de_tipo<L: AsRef<[u16]>>(ls: &[L], tipo: u8) -> bool {
    let de_tipo = |l: &[u16]| md::es_de_tipo(if tipo == 3 { l } else { &l[largo_cita(l)..] }, tipo);
    ls.iter().any(|l| !blanca(l.as_ref())) && ls.iter().all(|l| blanca(l.as_ref()) || de_tipo(l.as_ref()))
}

/// 0 viñetas, 1 numerada, 2 casillas, 3 cita. Si todas las líneas ya la tienen, se quita. Una lista va dentro de su
/// cita y en lugar de la marca de título: «- ## texto» se vería con las almohadillas. La marca de lista va en la primera
/// línea de cada párrafo juntado: las demás lo siguen dentro del elemento; la de cita, en todas.
fn listar(tipo: u8) {
    prefijar(false, |mut ls, sigue| {
        let primeras: Vec<&Vec<u16>> = ls.iter().zip(&sigue).filter(|x| tipo == 3 || !x.1).map(|x| x.0).collect();
        let todas = todas_de_tipo(&primeras, tipo);
        let varias = primeras.len() > 1;
        let mut num = 1;
        for (l, &s) in ls.iter_mut().zip(&sigue) {
            if blanca(l) && varias || s && tipo != 3 {
                continue;
            }
            if tipo == 3 {
                *l = if todas { md::quitar_cita(l) } else { [u("> "), l.clone()].concat() };
                continue;
            }
            let q = largo_cita(l);
            let r = &l[q..];
            let r = md::prefijo_titulo(r).map_or(r, |(_, m)| &r[m..]);
            let (sangria, resto) = match md::marca_lista(r) {
                Some((s, m)) => (&r[..s], &r[m..]),
                None => r.split_at(sangria_de(r)),
            };
            let marca = match tipo {
                _ if todas => Vec::new(),
                0 => u("- "),
                1 => {
                    num += 1;
                    u(&format!("{}. ", num - 1))
                }
                _ => u("- [ ] "),
            };
            *l = [&l[..q], sangria, &marca, resto].concat();
        }
        ls
    });
}

const TABLA: &str = "| Columna 1 | Columna 2 | Columna 3 |\r| --------- | --------- | --------- |\r|           |           |           |";

/// Fuera de una tabla, pone una debajo de la línea del cursor (nunca dentro de ella ni en lugar de lo seleccionado),
/// separada por líneas en blanco, y deja seleccionado «Columna 1». Dentro de una tabla, la amplía.
fn insertar_tabla() {
    let ed = app().ed.get();
    let t = texto();
    let (a, b) = editor::seleccion(ed);
    let b = b.min(t.len());
    let a = a.min(b);
    // Todo con la última línea seleccionada, que es tras la que va la tabla.
    let ultimo = if b > a { b - 1 } else { a };
    if lineas_md(&t, ultimo, ultimo).2[0].1.en_codigo() {
        return;
    }
    if let Some(tabla) = tabla_en(&t, inicio_linea(&t, ultimo)) {
        ampliar_tabla(&t, tabla, ultimo);
        return;
    }
    let (ini, fin) = (inicio_linea(&t, ultimo), fin_linea(&t, ultimo));
    let vacia = blanca(&t[ini..fin]);
    let encima = ini > 0 && !blanca(&t[inicio_linea(&t, ini - 1)..ini - 1]);
    let debajo_vacia = fin < t.len() && blanca(&t[fin + 1..fin_linea(&t, fin + 1)]);
    let antes = if !vacia { u("\r\r") } else if encima { u("\r") } else { Vec::new() };
    let mut tabla = [antes.clone(), u(TABLA)].concat();
    if !debajo_vacia {
        tabla.push(CR);
    }
    let desde = if vacia { ini } else { fin };
    editor::seleccionar(ed, desde, fin);
    editor::sustituir(ed, &tabla);
    let p = desde + antes.len() + 2;
    editor::seleccionar(ed, p, p + 9);
}

/// La tabla en que está la línea que empieza en `ini`: dónde empieza, dónde acaba (sin su salto) y en qué fila está.
fn tabla_en(t: &[u16], ini: usize) -> Option<(usize, usize, usize)> {
    let fila = |p: usize| md::fila(&t[p..fin_linea(t, p)]);
    if !fila(ini) {
        return None;
    }
    // Como mucho MAX_BLOQUE líneas hacia cada lado: se mira en cada tecla, y un bloque enorme de líneas con | (una
    // salida de un programa) no es una tabla con la que trabajar.
    let (mut desde, mut k) = (ini, 0);
    while desde > 0 && fila(inicio_linea(t, desde - 1)) {
        desde = inicio_linea(t, desde - 1);
        k += 1;
        if k > MAX_BLOQUE {
            return None;
        }
    }
    let (mut hasta, mut n) = (fin_linea(t, ini), 0);
    while hasta < t.len() && fila(hasta + 1) {
        hasta = fin_linea(t, hasta + 1);
        n += 1;
        if n > MAX_BLOQUE {
            return None;
        }
    }
    let segunda = fin_linea(t, desde) + 1;
    (segunda < hasta && md::plegable(&t[segunda..fin_linea(t, segunda)])).then_some((desde, hasta, k))
}

/// «Tabla» dentro de una tabla: una columna a la derecha de la del cursor, con «Columna N» (la primera que no está)
/// seleccionado en la cabecera, para escribir encima, y vacía en las demás filas. Nunca una columna sin nombre. La
/// tabla queda alineada, en un solo paso de deshacer. Decidido por Claude el 10/11, se puede cambiar.
fn ampliar_tabla(t: &[u16], (desde, hasta, _): (usize, usize, usize), cursor: usize) {
    let ed = app().ed.get();
    let viejo = &t[desde..hasta];
    let mut filas: Vec<Vec<Vec<u16>>> = viejo.split(|&c| c == CR).map(md::celdas).collect();
    let n = filas[0].len();
    let cabecera: Vec<Vec<u16>> = filas[0].iter().map(|c| c.iter().copied().filter(|&x| x != b' ' as u16).collect()).collect();
    let nombre = (1..).map(|i| u(&format!("Columna {i}"))).find(|x| !cabecera.contains(&x.iter().copied().filter(|&c| c != b' ' as u16).collect())).unwrap_or_default();
    // La celda del cursor: cuántas rayas sin escapar tiene delante en su fila.
    let ini = inicio_linea(t, cursor);
    let rayas = (ini..cursor.min(t.len())).filter(|&i| t[i] == b'|' as u16 && (i == ini || t[i - 1] != b'\\' as u16)).count();
    let nueva = rayas.clamp(1, n);
    for (i, f) in filas.iter_mut().enumerate() {
        if f.len() < n {
            f.resize(n, Vec::new());
        }
        f.insert(
            nueva.min(f.len()),
            match i {
                0 => nombre.clone(),
                1 => u("---"),
                _ => Vec::new(),
            },
        );
    }
    let sangria = &viejo[..sangria_de(viejo)];
    let juntas = filas
        .iter()
        .map(|f| {
            let mut l = sangria.to_vec();
            l.push(b'|' as u16);
            for c in f {
                l.push(b' ' as u16);
                l.extend_from_slice(c);
                l.extend([b' ' as u16, b'|' as u16]);
            }
            l
        })
        .collect::<Vec<_>>()
        .join(&CR);
    let nuevo = md::alinear_tablas(&juntas).unwrap_or(juntas);
    cambiar(desde, viejo, &nuevo);
    let cabeza = &nuevo[..nuevo.iter().position(|&c| c == CR).unwrap_or(nuevo.len())];
    // El nombre, en la celda nueva de la cabecera: tras la raya número `nueva`.
    let p = (0..cabeza.len()).filter(|&i| cabeza[i] == b'|' as u16 && (i == 0 || cabeza[i - 1] != b'\\' as u16)).nth(nueva).unwrap_or(0) + 2;
    editor::seleccionar(ed, desde + p, desde + p + nombre.len());
}

fn alinear_tablas() {
    let ed = app().ed.get();
    let t = texto();
    let Some(nuevo) = md::alinear_tablas(&t) else {
        poner_mensaje(crate::textos::t().sin_tablas);
        return;
    };
    let s = editor::seleccion(ed).0.min(t.len());
    let linea = t[..s].iter().filter(|&&c| c == CR).count();
    let ini = inicio_linea(&t, s);
    let vieja = &t[ini..fin_linea(&t, s)];
    cambiar(0, &t, &nuevo);
    let ini_n = nuevo.split(|&c| c == CR).take(linea).map(|l| l.len() + 1).sum::<usize>().min(nuevo.len());
    let pos = ini_n + misma_celda(vieja, s - ini, &nuevo[ini_n..fin_linea(&nuevo, ini_n)]);
    editor::seleccionar(ed, pos, pos);
    editor::ver_cursor(ed);
    poner_mensaje(crate::textos::t().tablas_alineadas);
}

/// Dónde cae en la fila `nueva` (la misma, ya alineada) lo que estaba en la columna `col` de `vieja`: en la misma celda
/// y a la misma distancia del principio de su texto. Fuera de una tabla, en la misma columna.
fn misma_celda(vieja: &[u16], col: usize, nueva: &[u16]) -> usize {
    let rayas = |l: &[u16]| (0..l.len()).filter(|&i| l[i] == b'|' as u16 && (i == 0 || l[i - 1] != b'\\' as u16)).collect::<Vec<_>>();
    let (rv, rn) = (rayas(vieja), rayas(nueva));
    let k = rv.iter().filter(|&&i| i < col).count();
    if !md::fila(vieja) || k == 0 || k > rn.len() {
        return col.min(nueva.len());
    }
    let texto = |l: &[u16], r: usize| r + 1 + sangria_de(&l[r + 1..]);
    let (av, an) = (texto(vieja, rv[k - 1]), texto(nueva, rn[k - 1]));
    let celda = &nueva[an..rn.get(k).copied().unwrap_or(nueva.len()).max(an)];
    let largo = celda.len() - celda.iter().rev().take_while(|&&c| c == b' ' as u16).count();
    an + col.saturating_sub(av).min(largo)
}

/// Intro: en una lista la sigue y en un elemento vacío la termina; en una cita, igual. En una tabla, pone una fila
/// debajo. Mayús+Intro es un salto normal (RichEdit pondría uno dentro del párrafo, que no es un salto de verdad en el
/// archivo). Con las líneas juntadas (ver md::salto_blando), un salto solo no se vería: Intro en un párrafo deja una
/// línea en blanco, que lo parte de verdad, y Mayús+Intro pone dos espacios delante del salto, que en Markdown es un
/// salto dentro del párrafo. Decidido por Claude el 11/11, se puede cambiar. Devuelve si lo ha atendido.
pub fn intro() -> bool {
    let a = app();
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let juntando = a.vivo.get() && editor::junta() && a.cfg.borrow().ajuste && !a.cfg.borrow().marcas;
    let de_parrafo = || {
        let t = a.copia.borrow();
        let ini = inicio_linea(&t, s.min(t.len()));
        md::de_parrafo(&t[ini..fin_linea(&t, ini)]) && abierto_en(&t, ini).is_none()
    };
    // Mayús+Intro es el salto de siempre, también en una lista.
    if pulsada(VK_SHIFT) {
        editor::sustituir(ed, if juntando && de_parrafo() { &[32, 32, CR][..] } else { &[CR][..] });
        return true;
    }
    if !a.md.get() {
        return intro_sangrado(ed, s);
    }
    let t = texto();
    let s = s.min(t.len());
    let ini = inicio_linea(&t, s);
    let fin = fin_linea(&t, s);
    // Dentro de un bloque de código, «- x» no es una lista; la sangría sí se conserva.
    if abierto_en(&t, ini).is_some() {
        return intro_sangrado(ed, s);
    }
    // En una fila de tabla, en cualquier sitio y aunque haya algo seleccionado en ella (recién puesta, «Columna 1»):
    // partirla rompería la tabla.
    if e <= fin
        && let Some(tabla) = tabla_en(&t, ini)
    {
        return intro_tabla(&t, ini, fin, tabla);
    }
    if s != e {
        // Lo seleccionado en un párrafo juntado se cambia por una línea en blanco, como Intro sin selección.
        if juntando && md::de_parrafo(&t[ini..fin]) {
            editor::sustituir(ed, &[CR, CR]);
            return true;
        }
        return false;
    }
    // En una línea que sigue un párrafo juntado, la lista o la cita son las de su primera línea.
    let primera = bloques_enteros(ed, &t, ini, ini).0;
    let antes = if primera < ini { &t[primera..fin_linea(&t, primera)] } else { &t[ini..s] };
    let q = largo_cita(antes);
    let (marca, siguiente) = match md::seguir_lista(&antes[q..]) {
        Some(sg) => (q + sg.largo, [&antes[..q], sg.siguiente.as_slice()].concat()),
        None if q > 0 => (q, antes[..q].to_vec()),
        None if juntando && md::de_parrafo(&t[ini..fin]) => {
            editor::sustituir(ed, if s > ini { &[CR, CR][..] } else { &[CR][..] });
            return true;
        }
        None => return intro_sangrado(ed, s),
    };
    let sangria = antes.iter().take_while(|&&c| c == b' ' as u16 || c == 9).count();
    if primera == ini && blanca(&t[(ini + marca).min(fin)..fin]) {
        // Un elemento vacío de un subnivel sube al nivel de encima, como uno más de esa lista (como Word).
        let padre = t[..ini.saturating_sub(1)].rsplit(|&c| c == CR).take(1000).filter(|l| !blanca(l)).find(|l| l.iter().take_while(|&&c| c == b' ' as u16 || c == 9).count() < sangria);
        if q == 0
            && sangria > 0
            && ini > 0
            && let Some(sg) = padre.and_then(md::seguir_lista)
        {
            let (nuevas, cola, hasta) = renumerar(&t, ini, fin, vec![sg.siguiente], &[0]);
            let c = ini + nuevas[0].len();
            cambiar(ini, &t[ini..hasta], &[nuevas.join(&CR), cola].concat());
            editor::seleccionar(ed, c, c);
            return true;
        }
        // En el primer nivel termina la lista (y se queda en la cita, si estaba en una); una cita vacía, la cita.
        editor::seleccionar(ed, if marca > q { ini + q } else { ini }, fin);
        editor::sustituir(ed, &[]);
    } else if q == 0 && md::es_de_tipo(&siguiente, 1) {
        // En una numerada, los de detrás corren uno.
        let c = s + 1 + siguiente.len();
        let nuevas = vec![t[ini..s].to_vec(), [siguiente, t[s..fin].to_vec()].concat()];
        let (nuevas, cola, hasta) = renumerar(&t, ini, fin, nuevas, &[1]);
        cambiar(ini, &t[ini..hasta], &[nuevas.join(&CR), cola].concat());
        editor::seleccionar(ed, c, c);
    } else if juntando && marca == q {
        // En una cita, el párrafo siguiente, tras una línea de la cita en blanco.
        let vacia = &antes[..q - antes[..q].iter().rev().take_while(|&&c| c == b' ' as u16).count()];
        editor::sustituir(ed, &[&[CR][..], vacia, &[CR], &siguiente].concat());
    } else {
        editor::sustituir(ed, &[vec![CR], siguiente].concat());
    }
    true
}

/// Intro en una fila de tabla: una fila vacía debajo (tras la |---| si es la cabecera), con las celdas del ancho de las
/// de la fila del cursor y el cursor en la primera. En una fila vacía al final de la tabla, se sale: la fila se borra.
/// Decidido por Claude el 09/11, se puede cambiar.
fn intro_tabla(t: &[u16], ini: usize, fin: usize, (_, hasta, k): (usize, usize, usize)) -> bool {
    let ed = app().ed.get();
    let l = &t[ini..fin];
    // En una fila vacía se sale: la fila se quita y el cursor pasa a la línea vacía de debajo de la tabla (si no la hay,
    // se pone), como en las listas. En cualquier fila del cuerpo, no solo en la última.
    if k >= 2 && md::celdas(l).iter().all(|c| blanca(c)) {
        if fin == hasta {
            editor::seleccionar(ed, ini, fin);
            editor::sustituir(ed, &[]);
            return true;
        }
        let resto = &t[fin + 1..hasta];
        let hay_vacia = hasta < t.len() && blanca(&t[hasta + 1..fin_linea(t, hasta + 1)]);
        let nuevo = if hay_vacia { resto.to_vec() } else { [resto, &[CR]].concat() };
        cambiar(ini, &t[ini..hasta], &nuevo);
        let c = ini + resto.len() + 1;
        editor::seleccionar(ed, c, c);
        return true;
    }
    fila_nueva(t, ini, fin, k);
    true
}

/// Una fila vacía debajo de la que va de `ini` a `fin` (tras la |---| si es la cabecera), con las celdas del mismo
/// ancho, y el cursor en su primera celda.
fn fila_nueva(t: &[u16], ini: usize, fin: usize, k: usize) {
    let ed = app().ed.get();
    let l = &t[ini..fin];
    let tras = if k == 0 { fin_linea(t, fin + 1) } else { fin };
    let raya = |i: usize| l[i] == b'|' as u16 && (i == 0 || l[i - 1] != b'\\' as u16);
    let fila: Vec<u16> = std::iter::once(CR).chain((0..l.len()).map(|i| if raya(i) { l[i] } else { b' ' as u16 })).collect();
    // Tras la primera raya y su espacio (el salto va delante).
    let primera = fila.iter().position(|&c| c == b'|' as u16).map_or(1, |p| (p + 2).min(fila.len()));
    editor::seleccionar(ed, tras, tras);
    editor::sustituir(ed, &fila);
    editor::seleccionar(ed, tras + primera, tras + primera);
}

/// Tab y Mayús+Tab en una tabla: a la celda siguiente o a la anterior, con su texto seleccionado (si está vacía, el
/// cursor en ella); la fila |---| se salta. Tab en la última celda pone una fila vacía debajo. Devuelve si el cursor
/// estaba en una tabla. Decidido por Claude el 10/11, se puede cambiar.
pub fn celda(adelante: bool) -> bool {
    let a = app();
    if !a.md.get() {
        return false;
    }
    let ed = a.ed.get();
    let t = texto();
    let s = editor::seleccion(ed).0.min(t.len());
    let ini = inicio_linea(&t, s);
    if abierto_en(&t, ini).is_some() {
        return false;
    }
    let Some((desde, hasta, _)) = tabla_en(&t, ini) else { return false };
    // Cada celda: dónde empieza (tras su raya), dónde acaba (en la raya siguiente) y su texto sin los espacios de
    // alrededor; y de qué fila es.
    let mut celdas = Vec::new();
    let (mut l, mut k) = (desde, 0);
    while l <= hasta {
        let f = fin_linea(&t, l);
        let rayas: Vec<usize> = (l..f).filter(|&i| t[i] == b'|' as u16 && (i == l || t[i - 1] != b'\\' as u16)).collect();
        if k != 1 {
            for par in rayas.windows(2) {
                let (x, y) = (par[0] + 1, par[1]);
                let a = x + t[x..y].iter().take_while(|&&c| c == b' ' as u16).count();
                let b = (a..y).rev().find(|&i| t[i] != b' ' as u16).map_or(a, |i| i + 1);
                celdas.push((x, y, a.min(y), b.max(a), l, f, k));
            }
        }
        l = f + 1;
        k += 1;
    }
    let Some(j) = celdas.iter().rposition(|c| c.0 <= s).or(Some(0)).filter(|_| !celdas.is_empty()) else { return true };
    let destino = if adelante { j + 1 } else { j.wrapping_sub(1) };
    match celdas.get(destino) {
        Some(&(x, y, a, b, ..)) if a == b => editor::seleccionar(ed, (x + 1).min(y), (x + 1).min(y)),
        Some(&(_, _, a, b, ..)) => editor::seleccionar(ed, a, b),
        None if adelante => {
            let &(.., l, f, k) = celdas.last().unwrap_or(&(0, 0, 0, 0, desde, hasta, 2));
            fila_nueva(&t, l, f, k);
        }
        None => {}
    }
    true
}

// --- Ver ---

fn colores() {
    let a = app();
    // Las transparentes llevan el fondo de antes.
    soltar_imagenes();
    let noche = a.cfg.borrow().noche;
    if !a.vista.get() {
        unsafe { SetPropW(a.hwnd.get(), PROP_NOCHE.as_ptr(), (1 + noche as usize) as HANDLE) };
    }
    a.barras.noche.set(noche);
    tema::titulo(a.hwnd.get(), noche);
    tema::nativo(a.ed.get(), noche, "DarkMode_Explorer");
    aplicar_formato_base();
    if a.vivo.get() {
        pintar_todo();
    }
    // La vista previa no las tiene: con una ventana nula se repintaría todo el escritorio.
    for h in [a.botones.get(), a.estado.get()] {
        if !h.is_null() {
            unsafe { InvalidateRect(h, std::ptr::null(), 0) };
        }
    }
}

pub fn zoom(paso: i32) {
    let a = app();
    let z = if paso == 0 { 100 } else { (a.cfg.borrow().zoom + paso * 10).clamp(30, 500) };
    if z == a.cfg.borrow().zoom {
        return;
    }
    a.cfg.borrow_mut().zoom = z;
    editor::zoom(a.ed.get(), z);
    estado();
}

fn elegir_letra() {
    let a = app();
    if a.vivo.get() {
        poner_mensaje(t().solo_texto);
        return;
    }
    let mut lf: LOGFONTW = unsafe { std::mem::zeroed() };
    {
        let cfg = a.cfg.borrow();
        // Redondeado aquí y a la vuelta: truncando, 11 puntos a 96 ppp salían 10,5 y Aceptar dejaba 10.
        lf.lfHeight = -((cfg.tamano * barras::dpi() + 36) / 72);
        lf.lfCharSet = 1;
        for (i, ch) in cfg.letra.encode_utf16().take(31).enumerate() {
            lf.lfFaceName[i] = ch;
        }
    }
    let mut cf: CHOOSEFONTW = unsafe { std::mem::zeroed() };
    cf.lStructSize = std::mem::size_of::<CHOOSEFONTW>() as u32;
    cf.hwndOwner = a.hwnd.get();
    cf.lpLogFont = &mut lf;
    cf.Flags = CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT | CF_NOVERTFONTS | CF_NOSCRIPTSEL;
    if unsafe { ChooseFontW(&mut cf) } == 0 {
        return;
    }
    let n = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(32);
    let letra = String::from_utf16_lossy(&lf.lfFaceName[..n]);
    let tamano = ((cf.iPointSize + 5) / 10).clamp(6, 72);
    {
        let mut cfg = a.cfg.borrow_mut();
        // Volver a dar formato a un archivo grande cuesta segundos: solo si de verdad cambia.
        if cfg.letra == letra && cfg.tamano == tamano {
            return;
        }
        cfg.letra = letra;
        cfg.tamano = tamano;
    }
    aplicar_formato_base();
}

// --- Menús ---

fn estados_menu(m: HMENU) {
    let a = app();
    let ed = a.ed.get();
    let (s, e) = editor::seleccion(ed);
    let sel = s != e;
    let (atras, adelante) = if a.vivo.get() {
        let h = a.historial.borrow();
        (h.puede_deshacer(), h.puede_rehacer())
    } else {
        (enviar(ed, EM_CANUNDO, 0, 0) != 0, enviar(ed, re::EM_CANREDO, 0, 0) != 0)
    };
    // Texto o archivos copiados en el Explorador (CF_HDROP).
    let pegar = unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT as u32) != 0 || IsClipboardFormatAvailable(15) != 0 };
    let activo = |si: bool| if si { MF_ENABLED } else { MF_GRAYED };
    let cfg = a.cfg.borrow();
    unsafe {
        EnableMenuItem(m, c::DESHACER as u32, activo(atras));
        EnableMenuItem(m, c::REHACER as u32, activo(adelante));
        for id in [c::CORTAR, c::COPIAR, c::ELIMINAR] {
            EnableMenuItem(m, id as u32, activo(sel));
        }
        EnableMenuItem(m, c::PEGAR as u32, activo(pegar));
        EnableMenuItem(m, c::PEGAR_TEXTO as u32, activo(pegar));
        EnableMenuItem(m, c::LETRA as u32, activo(!a.vivo.get()));
        EnableMenuItem(m, c::MARCAS as u32, activo(a.vivo.get()));
        for (id, si) in [(c::AJUSTE, cfg.ajuste), (c::BARRA, cfg.barra), (c::NOCHE, cfg.noche), (c::CORRECTOR, cfg.corrector), (c::MARCAS, cfg.marcas), (c::NUEVOS_MD, cfg.nuevos_md), (c::CONTRARIO, cfg.contrario), (c::COPIA, cfg.copia)] {
            CheckMenuItem(m, id as u32, if si { MF_CHECKED } else { MF_UNCHECKED });
        }
        let cod = match a.cod.get() {
            Cod::Utf8 => 0,
            Cod::Utf8Bom => 1,
            Cod::Utf16 { be: false, .. } => 2,
            Cod::Utf16 { be: true, .. } => 3,
            Cod::Ansi(_) => 4,
        };
        CheckMenuRadioItem(a.menu_cod.get(), c::COD as u32, c::COD as u32 + 4, c::COD as u32 + cod, MF_BYCOMMAND);
        let saltos = match a.saltos.get() {
            Saltos::Crlf => 0,
            Saltos::Lf => 1,
            Saltos::Cr => 2,
        };
        CheckMenuRadioItem(a.menu_saltos.get(), c::SALTOS as u32, c::SALTOS as u32 + 2, c::SALTOS as u32 + saltos, MF_BYCOMMAND);
        let m = c::MARGENES as u32;
        CheckMenuRadioItem(a.menu_margenes.get(), m, m + 3, m + cfg.margenes as u32, MF_BYCOMMAND);
    }
}

pub fn menu_contextual(lp: LPARAM) {
    let a = app();
    let t = t();
    let (mut x, mut y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    if lp == -1 {
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            GetCaretPos(&mut p);
            ClientToScreen(a.ed.get(), &mut p);
        }
        (x, y) = (p.x, p.y);
    }
    unsafe {
        let m = CreatePopupMenu();
        anadir(m, c::DESHACER, t.deshacer);
        raya(m);
        for (id, s) in [(c::CORTAR, t.cortar), (c::COPIAR, t.copiar), (c::PEGAR, t.pegar), (c::ELIMINAR, t.eliminar)] {
            anadir(m, id, s);
        }
        raya(m);
        anadir(m, c::TODO, t.seleccionar_todo);
        estados_menu(m);
        TrackPopupMenuEx(m, TPM_RIGHTBUTTON, x, y, a.hwnd.get(), std::ptr::null());
        DestroyMenu(m);
    }
}

/// Clic en la barra de estado: en los saltos o la codificación, su menú para cambiarlos.
pub fn menu_estado(parte: usize, p: POINT) {
    let a = app();
    let m = match parte {
        0 if crate::informe::avisado() => return crate::informe::preguntar(),
        3 => a.menu_saltos.get(),
        4 => a.menu_cod.get(),
        _ => return,
    };
    estados_menu(a.menu.get());
    unsafe { TrackPopupMenuEx(m, TPM_BOTTOMALIGN | TPM_LEFTALIGN, p.x, p.y, a.hwnd.get(), std::ptr::null()) };
}

// Windows oscurece los menús desplegables, pero no la barra de menús: en modo noche la pinta Bloc.
#[repr(C)]
struct UahMenu {
    hmenu: HMENU,
    hdc: HDC,
    flags: u32,
}

#[repr(C)]
struct UahDrawMenuItem {
    dis: DRAWITEMSTRUCT,
    um: UahMenu,
    posicion: i32,
}

/// La letra de la barra de menús, creada la primera vez que se pinta y no en cada menú.
fn letra_menu() -> HFONT {
    thread_local!(static LETRA: Cell<HFONT> = const { Cell::new(std::ptr::null_mut()) });
    LETRA.with(|l| {
        if l.get().is_null() {
            let mut ncm: NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
            ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
            unsafe { SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut _, 0) };
            l.set(unsafe { CreateFontIndirectW(&ncm.lfMenuFont) });
        }
        l.get()
    })
}

fn menus_oscuros(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let a = app();
    let p = tema::paleta(true);
    unsafe {
        match m {
            0x91 => {
                // WM_UAHDRAWMENU: el fondo de la barra
                let um = &*(lp as *const UahMenu);
                let mut mb: MENUBARINFO = std::mem::zeroed();
                mb.cbSize = std::mem::size_of::<MENUBARINFO>() as u32;
                let mut w = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                if GetMenuBarInfo(h, OBJID_MENU, 0, &mut mb) == 0 || GetWindowRect(h, &mut w) == 0 {
                    a.menus_fallan.set(true);
                    return DefWindowProcW(h, m, wp, lp);
                }
                let r = RECT { left: mb.rcBar.left - w.left, top: mb.rcBar.top - w.top, right: mb.rcBar.right - w.left, bottom: mb.rcBar.bottom - w.top };
                let b = CreateSolidBrush(p.barra);
                FillRect(um.hdc, &r, b);
                DeleteObject(b as _);
                1
            }
            0x92 => {
                // WM_UAHDRAWMENUITEM: cada menú
                let d = &*(lp as *const UahDrawMenuItem);
                let mut s = [0u16; 64];
                let n = GetMenuStringW(d.um.hmenu, d.posicion as u32, s.as_mut_ptr(), 64, MF_BYPOSITION);
                let e = d.dis.itemState;
                let mut r = d.dis.rcItem;
                let b = CreateSolidBrush(if e & (ODS_SELECTED | ODS_HOTLIGHT) != 0 { p.resalte } else { p.barra });
                FillRect(d.um.hdc, &r, b);
                DeleteObject(b as _);
                let mut f = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
                if e & ODS_NOACCEL != 0 {
                    f |= DT_HIDEPREFIX; // sin subrayar hasta pulsar Alt
                }
                SetBkMode(d.um.hdc, TRANSPARENT as i32);
                SetTextColor(d.um.hdc, if e & (ODS_GRAYED | ODS_DISABLED) != 0 { p.apagado } else { p.texto });
                let antes = SelectObject(d.um.hdc, letra_menu() as _);
                DrawTextW(d.um.hdc, s.as_ptr(), n.max(0), &mut r, f);
                SelectObject(d.um.hdc, antes);
                1
            }
            _ => {
                // WM_NCPAINT y WM_NCACTIVATE: Windows deja una raya clara bajo la barra.
                let r = DefWindowProcW(h, m, wp, lp);
                let mut c = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                let mut w = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(h, &mut c);
                let mut o = POINT { x: 0, y: 0 };
                ClientToScreen(h, &mut o);
                GetWindowRect(h, &mut w);
                let dc = GetWindowDC(h);
                let b = CreateSolidBrush(p.barra);
                let linea = RECT { left: o.x - w.left, top: o.y - w.top - 1, right: o.x - w.left + c.right, bottom: o.y - w.top };
                FillRect(dc, &linea, b);
                DeleteObject(b as _);
                ReleaseDC(h, dc);
                r
            }
        }
    }
}

// --- Título y barra de estado ---

fn titulo() {
    let a = app();
    let t = ws(&format!("{}{} - Bloc", if a.cambiado.get() { "*" } else { "" }, nombre()));
    unsafe { SetWindowTextW(a.hwnd.get(), t.as_ptr()) };
}

fn marcar() {
    let a = app();
    a.rev.set(a.rev.get() + 1);
    // Un documento nuevo que se vuelve a dejar vacío no tiene nada que guardar.
    let cambiado = a.ruta.borrow().is_some() || largo() > 0;
    if a.cambiado.get() != cambiado {
        a.cambiado.set(cambiado);
        titulo();
    }
    if cambiado && a.cfg.borrow().copia {
        crate::copia::cambio(a.hwnd.get());
    }
}

pub fn poner_mensaje(s: &str) {
    if s != t().informar_fallo_antes {
        crate::informe::olvidar_aviso();
    }
    let h = app().estado.get();
    // Sin barra (la vista previa), nada: con una ventana nula se repintaría la pantalla entera.
    if !h.is_null() {
        barras::poner(h, 0, s);
    }
}

/// Línea real (no la visual del ajuste) y columna, siguiendo desde la última línea calculada en vez de contar desde
/// el principio: al teclear o moverse, el recorrido es corto aunque el archivo sea grande.
fn posicion(pos: usize) -> (usize, usize) {
    let a = app();
    let mut m = a.marca.get();
    let (ini, _, hasta) = m;
    if ini > 0 && (ini > largo() || trozo(ini - 1, ini) != [CR]) || hasta > largo() {
        m = (0, 1, 0);
    }
    let (ini, lin, hasta) = situar(m, pos, trozo);
    a.marca.set((ini, lin, hasta));
    (lin, pos - ini + 1)
}

/// La marca de `posicion` tras llevarla a `pos`, leyendo con `leer` solo lo que aún no se sabe.
fn situar((mut ini, mut lin, hasta): (usize, usize, usize), pos: usize, leer: impl Fn(usize, usize) -> Vec<u16>) -> (usize, usize, usize) {
    if pos >= hasta {
        let t = leer(hasta, pos);
        lin += t.iter().filter(|&&ch| ch == CR).count();
        if let Some(k) = t.iter().rposition(|&ch| ch == CR) {
            ini = hasta + k + 1;
        }
    } else if pos < ini {
        let t = leer(pos, ini);
        lin -= t.iter().filter(|&&ch| ch == CR).count();
        // El inicio de esa línea: hacia atrás a trozos.
        let mut fin = pos;
        ini = loop {
            let desde = fin.saturating_sub(4096);
            let t = leer(desde, fin);
            if let Some(k) = t.iter().rposition(|&ch| ch == CR) {
                break desde + k + 1;
            }
            if desde == 0 {
                break 0;
            }
            fin = desde;
        };
    } else {
        return (ini, lin, hasta); // en la parte ya sabida de la misma línea: no hace falta leer nada
    }
    (ini, lin, pos)
}

/// La marca de `posicion` tras un cambio que empieza en `p`: lo de antes de `p` sigue igual. Lo que empieza justo al
/// principio de la línea marcada no mueve ese principio.
fn tras_cambio((ini, lin, hasta): (usize, usize, usize), p: usize) -> (usize, usize, usize) {
    if p < ini { (0, 1, 0) } else { (ini, lin, hasta.min(p)) }
}

pub fn estado() {
    let a = app();
    if !a.cfg.borrow().barra {
        return;
    }
    let (s, e) = editor::seleccion(a.ed.get());
    let (lin, col) = posicion(s);
    let t = t();
    let mut p = con(&con(t.lin_col, lin), col);
    if e > s {
        p += "   ";
        p += &con(t.seleccionados, e - s);
    }
    let h = a.estado.get();
    barras::poner(h, 1, &p);
    barras::poner(h, 2, &format!("{} %", a.cfg.borrow().zoom));
    barras::poner(h, 3, nombre_saltos(a.saltos.get()));
    barras::poner(h, 4, &nombre_cod(a.cod.get()));
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn imagenes_solo_de_este_equipo() {
        let doc = Path::new(r"C:\notas\a.md");
        let ruta = |s: &str| ruta_de_imagen(s, Some(doc));
        assert_eq!(ruta("![](fotos/b%20c.png)"), Some(PathBuf::from(r"C:\notas\fotos\b c.png")));
        assert_eq!(ruta("![](../x.jpg)"), Some(PathBuf::from(r"C:\x.jpg")));
        assert_eq!(ruta(r"![](D:\y.gif)"), Some(PathBuf::from(r"D:\y.gif")));
        for mala in [r"![](\\otro\c\x.png)", "![](//otro/c/x.png)", r"![](\\?\UNC\otro\c\x.png)", r"![](\\.\pipe\x.png)", "![](https://a.es/x.png)", "![](x.emf)", "![](x)"] {
            assert_eq!(ruta(mala), None, "{mala}");
        }
        let en_red = Path::new(r"\\Nas\datos\a.md");
        assert_eq!(ruta_de_imagen("![](b.png)", Some(en_red)), Some(PathBuf::from(r"\\Nas\datos\b.png")));
        assert_eq!(ruta_de_imagen(r"![](\\nas\otra\b.png)", Some(en_red)), Some(PathBuf::from(r"\\nas\otra\b.png")));
        assert_eq!(ruta_de_imagen(r"![](\\otro\c\b.png)", Some(en_red)), None);
        assert_eq!(ruta_de_imagen("![](b.png)", None), None);
    }

    #[test]
    fn lista_en_linea_vacia() {
        assert!(!todas_de_tipo(&[u("")], 0), "en una línea vacía, Viñetas pone la marca");
        assert!(!todas_de_tipo(&[u("  "), u("")], 1));
        assert!(todas_de_tipo(&[u("- a"), u(""), u("* b")], 0), "las vacías no cuentan");
        assert!(!todas_de_tipo(&[u("- [ ] a")], 0) && todas_de_tipo(&[u("- [ ] a")], 2));
    }

    #[test]
    fn posicion_al_azar() {
        let mut semilla: u64 = 11;
        let mut azar = |n: usize| {
            semilla = semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((semilla >> 33) % n as u64) as usize
        };
        let mut t: Vec<u16> = Vec::new();
        let mut m = (0, 1, 0);
        for _ in 0..20000 {
            if azar(3) == 0 {
                let p = azar(t.len() + 1);
                let q = (p + azar(5)).min(t.len());
                let puesto: Vec<u16> = (0..azar(6)).map(|_| if azar(4) == 0 { CR } else { b'a' as u16 }).collect();
                t.splice(p..q, puesto);
                m = tras_cambio(m, p);
            }
            let pos = azar(t.len() + 1);
            m = situar(m, pos, |i, f| t[i..f].to_vec());
            let lin = 1 + t[..pos].iter().filter(|&&c| c == CR).count();
            assert_eq!((m.0, m.1), (inicio_linea(&t, pos), lin));
            assert!(m.0 <= pos && pos <= m.2 && !t[m.0..m.2].contains(&CR), "lo sabido de la línea no tiene saltos");
        }
    }
}
