// Bloc.exe es también su instalador. Abierto desde otro sitio, pregunta y se copia (o se actualiza) en
// %LOCALAPPDATA%\Programs\Bloc, se pone en el menú Inicio, en «Abrir con» y en Aplicaciones instaladas, y abre la copia
// instalada. La instalada se pone al día sola desde la carpeta de donde se instaló.
use std::cell::{Cell, RefCell};
use std::ffi::{c_void, OsString};
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, LPARAM, S_FALSE, S_OK, WPARAM};
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW, RegEnumKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetValueExW, HKEY,
    HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_DWORD, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, REG_EXPAND_SZ,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, CREATE_NO_WINDOW, INFINITE};
use windows_sys::Win32::UI::Controls::{
    TASKDIALOGCONFIG, TASKDIALOG_BUTTON, TDCBF_CANCEL_BUTTON, TDCBF_CLOSE_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER, TDF_SHOW_MARQUEE_PROGRESS_BAR, TDM_CLICK_BUTTON,
    TDM_ENABLE_BUTTON, TDM_NAVIGATE_PAGE, TDM_SET_PROGRESS_BAR_MARQUEE, TDN_BUTTON_CLICKED, TDN_NAVIGATED, TDN_TIMER,
};
use windows_sys::Win32::UI::Shell::{SHChangeNotify, ShellExecuteExW, ShellExecuteW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, IDCANCEL, IDCLOSE, MB_ICONWARNING, MB_OK, SW_SHOWNORMAL, WM_APP};

use crate::com::Com;
use crate::previa::{CLSID_MD, CLSID_TEXTO, IID_VISTA};
use crate::textos::t;
use crate::w::{apuntar, con, enviar, funcion, mensaje, ws, ws_os};

const BLOC: &str = r"Software\Bloc";
const CLASES: &str = r"Software\Classes";
const DESINSTALAR: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Bloc";
const INICIO: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Las de siempre; la página de elegir añade las que el Bloc de notas de Windows abre en cada PC (`de_notas`).
pub const EXTENSIONES: [&str; 14] = [".txt", ".md", ".markdown", ".log", ".ini", ".inf", ".csv", ".json", ".xml", ".ps1", ".psd1", ".psm1", ".scp", ".wtx"];

// Las pruebas ponen BLOC_PROGRAMAS (otra carpeta para el programa) y BLOC_REGISTRO (una clave de HKCU bajo la que va
// todo lo que Bloc escribe en el registro): así no tocan el Bloc instalado de quien las pasa.
fn destino() -> Option<PathBuf> {
    let dir = match std::env::var_os("BLOC_PROGRAMAS") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs").join("Bloc"),
    };
    Some(dir.join("Bloc.exe"))
}

fn en_registro(ruta: &str) -> Vec<u16> {
    ws(&match std::env::var("BLOC_REGISTRO") {
        Ok(raiz) => format!(r"{raiz}\{ruta}"),
        Err(_) => ruta.to_string(),
    })
}

fn menu_inicio() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs\Bloc.lnk"))
}

/// Si no coinciden tal cual, resueltas: la misma ruta puede llegar escrita de otra forma (nombres cortos).
fn es(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str()) || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Primero el tamaño: así, abierto desde la carpeta de red, no lee los dos enteros si no hace falta.
fn iguales(a: &Path, b: &Path) -> bool {
    if !matches!((std::fs::metadata(a), std::fs::metadata(b)), (Ok(x), Ok(y)) if x.len() == y.len()) {
        return false;
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Un .exe abierto no se deja abrir para escribir.
fn en_uso(exe: &Path) -> bool {
    std::fs::OpenOptions::new().write(true).open(exe).is_err()
}

/// Una función de una DLL que solo hace falta al instalar o al buscar una versión nueva: se carga entonces, y no al
/// abrir Bloc.
/// La versión del recurso que pone sellar (la de Cargo.toml), en el formato de Windows: cada parte en 16 bits. Solo si
/// el .exe es de verdad un Bloc.
fn version_de(exe: &Path) -> Option<u64> {
    type Largo = unsafe extern "system" fn(*const u16, *mut u32) -> u32;
    type Leer = unsafe extern "system" fn(*const u16, u32, u32, *mut c_void) -> i32;
    type Valor = unsafe extern "system" fn(*const c_void, *const u16, *mut *const c_void, *mut u32) -> i32;
    let (largo, leer, valor): (Largo, Leer, Valor) = unsafe {
        (
            std::mem::transmute::<usize, Largo>(funcion("version.dll", "GetFileVersionInfoSizeW")?),
            std::mem::transmute::<usize, Leer>(funcion("version.dll", "GetFileVersionInfoW")?),
            std::mem::transmute::<usize, Valor>(funcion("version.dll", "VerQueryValueW")?),
        )
    };
    let r = ws_os(exe.as_os_str());
    let n = unsafe { largo(r.as_ptr(), &mut 0) };
    let mut datos = vec![0u8; n as usize];
    if n == 0 || unsafe { leer(r.as_ptr(), 0, n, datos.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let pedir = |que: &str| {
        let (q, mut p, mut l) = (ws(que), std::ptr::null(), 0);
        (unsafe { valor(datos.as_ptr().cast(), q.as_ptr(), &mut p, &mut l) } != 0 && !p.is_null()).then_some((p, l as usize))
    };
    let (p, l) = pedir(r"\StringFileInfo\040904B0\ProductName")?;
    if String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p as *const u16, l.min(16)) }).trim_end_matches('\0') != "Bloc" {
        return None;
    }
    let (p, l) = pedir("\\")?;
    if l < 16 {
        return None;
    }
    let fijo: [u32; 4] = std::array::from_fn(|i| unsafe { (p as *const u32).add(i).read_unaligned() });
    (fijo[0] == 0xFEEF04BD).then(|| (fijo[2] as u64) << 32 | fijo[3] as u64)
}

fn texto_version(v: u64) -> String {
    format!("{}.{}.{}", v >> 48, v >> 32 & 0xFFFF, v >> 16 & 0xFFFF)
}

/// La ruta de red de una unidad (A:\... → \\servidor\...), para que valga aunque en otro momento falte la letra.
fn universal(ruta: &Path) -> OsString {
    type Universal = unsafe extern "system" fn(*const u16, u32, *mut c_void, *mut u32) -> u32;
    let r = ws_os(ruta.as_os_str());
    let mut b = vec![0u64; 1024];
    let mut n = (b.len() * 8) as u32;
    // UNIVERSAL_NAME_INFO_LEVEL. Fuera de una unidad de red falla, y se queda la ruta tal cual.
    let hecha = funcion("mpr.dll", "WNetGetUniversalNameW").is_some_and(|f| unsafe { std::mem::transmute::<usize, Universal>(f)(r.as_ptr(), 1, b.as_mut_ptr().cast(), &mut n) } == 0);
    if !hecha {
        return ruta.as_os_str().to_owned();
    }
    // UNIVERSAL_NAME_INFOW: un puntero a la ruta, que va detrás en el mismo búfer.
    let p = b[0] as *const u16;
    let largo = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
    OsString::from_wide(unsafe { std::slice::from_raw_parts(p, largo) })
}

/// Devuelve true si este proceso ya no hace falta: ha abierto la copia instalada, o se ha cancelado.
pub fn instalar(args: &[OsString]) -> bool {
    let (Some(destino), Ok(yo)) = (destino(), std::env::current_exe()) else { return false };
    if es(&yo, &destino) {
        // Los que apartó una actualización con Bloc abierto, y la copia de una que se cortó.
        for e in std::fs::read_dir(destino.parent().unwrap_or(Path::new("."))).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            // La copia conserva la fecha de modificación del original: la de creación es la de cuando se copió.
            let reciente = e.metadata().is_ok_and(|m| [m.modified(), m.created()].into_iter().flatten().any(|f| f.elapsed().is_ok_and(|d| d.as_secs() < 600)));
            if n.starts_with("bloc.exe") && (n.ends_with(".viejo") || n.ends_with(".nuevo")) && !reciente {
                let _ = std::fs::remove_file(e.path());
            }
        }
        return false;
    }
    let (mia, suya) = (version_de(&yo), version_de(&destino));
    let hecho = match (destino.exists(), mia, suya) {
        (false, ..) => instalar_nuevo(&yo, &destino, mia, args),
        (true, Some(m), Some(s)) if m > s => actualizar_preguntando(&yo, &destino, m, s, args),
        (true, Some(m), Some(s)) if m < s && args.is_empty() => mas_nueva(&destino, m, s),
        (true, Some(m), Some(s)) if m < s => abrir(&destino, false, args),
        // La misma versión (o sin recurso de versión, recién compilado): como siempre, sin preguntar.
        _ => {
            let nuevo = !iguales(&yo, &destino);
            if nuevo {
                poner_en(&yo, &destino).and_then(|()| abrir(&destino, true, args))
            } else {
                // Ya igual, pero puesto sin pasar por aquí (a mano, o de otra carpeta antes de apuntarlo): sin el origen
                // no se buscan las nuevas, y sin la entrada no sale en Aplicaciones instaladas. Solo si faltan: abrir
                // una copia igual de otro sitio no cambia de dónde se actualiza.
                if leer(BLOC, "Origen").is_none() || leer(DESINSTALAR, "DisplayName").is_none() {
                    registrar(&destino, &yo);
                }
                abrir(&destino, false, args)
            }
        }
    };
    match hecho {
        Ok(()) => true,
        Err(e) => {
            apuntar(&format!("No se ha podido instalar: {e}"));
            mensaje(std::ptr::null_mut(), &format!("{}\n\n{e}", t().no_instalar), MB_OK | MB_ICONWARNING);
            false
        }
    }
}

fn instalar_nuevo(yo: &Path, destino: &Path, mia: Option<u64>, args: &[OsString]) -> io::Result<()> {
    let t = t();
    let version = mia.map_or_else(|| env!("CARGO_PKG_VERSION").to_string(), texto_version);
    let paginas = [Pagina::nueva(t.instalar_titulo, Some(&con(t.instalar_texto, version)), &[(SI, t.instalar_si)], TDCBF_CANCEL_BUTTON)];
    let (desde, a) = (yo.to_path_buf(), destino.to_path_buf());
    if dialogo(&paginas, move || poner_en(&desde, &a))? == IDCANCEL {
        return Ok(());
    }
    let (mut e, _) = eleccion();
    let ahora = ahora(&e.todas);
    let boton = crate::eleccion::preguntar(std::ptr::null_mut(), crate::tema::como_windows(), &mut e, &ahora);
    aplicar(&e);
    match boton {
        crate::eleccion::ABRIR => abrir(destino, true, args),
        // Cerrado sin abrir Bloc: «Nueva nota» en la barra de tareas la tiene que poner el instalado.
        _ => Command::new(destino).arg("/lista").spawn().map(drop),
    }
}

fn actualizar_preguntando(yo: &Path, destino: &Path, mia: u64, suya: u64, args: &[OsString]) -> io::Result<()> {
    let t = t();
    let mut texto = con(&con(t.actualizar_texto, texto_version(suya)), texto_version(mia));
    if en_uso(destino) {
        texto = format!("{texto}\n\n{}", t.actualizar_abierto);
    }
    let paginas = [Pagina::nueva(t.actualizar_titulo, Some(&texto), &[(SI, t.actualizar_si)], TDCBF_CANCEL_BUTTON)];
    let (desde, a) = (yo.to_path_buf(), destino.to_path_buf());
    match dialogo(&paginas, move || poner_en(&desde, &a))? {
        IDCANCEL => Ok(()),
        _ => abrir(destino, true, args),
    }
}

/// Nunca se instala una versión más antigua encima.
fn mas_nueva(destino: &Path, mia: u64, suya: u64) -> io::Result<()> {
    let t = t();
    let texto = con(&con(t.mas_nueva_texto, texto_version(suya)), texto_version(mia));
    let paginas = [Pagina::nueva(t.mas_nueva_titulo, Some(&texto), &[(ABRIR, t.abrir_bloc)], TDCBF_CANCEL_BUTTON)];
    match dialogo(&paginas, || Ok(()))? {
        ABRIR => abrir(destino, false, &[]),
        _ => Ok(()),
    }
}

fn abrir(exe: &Path, instalado: bool, args: &[OsString]) -> io::Result<()> {
    let mut orden = Command::new(exe);
    if instalado {
        orden.arg("/instalado");
    }
    orden.args(args).spawn().map(drop)
}

fn poner_en(yo: &Path, destino: &Path) -> io::Result<()> {
    std::fs::create_dir_all(destino.parent().unwrap_or(Path::new(".")))?;
    cambiar(&copia(yo, destino)?, destino)?;
    // Solo al instalar o actualizar: avisar al Explorador le hace rehacer los iconos.
    registrar(destino, yo);
    Ok(())
}

/// Se copia aparte para que una copia cortada no estropee el instalado. Con el número del proceso: dos instalaciones a
/// la vez (dos clics seguidos) no se pisan la copia.
fn copia(desde: &Path, destino: &Path) -> io::Result<PathBuf> {
    let tmp = destino.with_extension(format!("exe.{}.nuevo", std::process::id()));
    std::fs::copy(desde, &tmp)?;
    Ok(tmp)
}

/// Un Bloc abierto no deja sobrescribir su .exe, pero sí cambiarle el nombre; si el apartado de antes sigue abierto,
/// otro nombre.
fn cambiar(tmp: &Path, destino: &Path) -> io::Result<()> {
    let mut viejo = destino.with_extension("exe.viejo");
    if destino.exists() {
        let _ = std::fs::remove_file(&viejo);
        if viejo.exists() {
            viejo = destino.with_extension(format!("exe.{}.viejo", std::process::id()));
        }
        std::fs::rename(destino, &viejo)?;
    }
    if let Err(e) = std::fs::rename(tmp, destino) {
        if !destino.exists() && viejo.exists() {
            let _ = std::fs::rename(&viejo, destino);
        }
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    let _ = std::fs::remove_file(&viejo);
    Ok(())
}

// --- La actualización desde la carpeta de donde se instaló ---

/// La clave pública de las actualizaciones (ECDSA P-256, X e Y): la privada está en privado\firma, y construir.ps1
/// firma con ella cada Bloc.exe.
const CLAVE_PUBLICA: [u8; 64] = [
    0x97, 0x58, 0x4A, 0x9E, 0x77, 0x67, 0x44, 0x3A, 0xA7, 0xFB, 0xB4, 0xE3, 0x9D, 0x35, 0x25, 0x42, 0x17, 0xEF, 0xE6, 0x69, 0x4F, 0x1C, 0x6C, 0x83, 0xAA, 0x4A, 0x9E, 0x8D, 0x89, 0x11, 0xBD, 0x08,
    0x61, 0xDA, 0x02, 0x05, 0x1E, 0x22, 0xF4, 0xD3, 0x07, 0xC4, 0x91, 0x83, 0x94, 0x86, 0x67, 0xB6, 0xF2, 0x86, 0x61, 0x37, 0x17, 0xA0, 0xC2, 0xEF, 0xB8, 0x3C, 0x7F, 0xCE, 0x6F, 0x97, 0xA6, 0xF5,
];
const MARCA_FIRMA: &[u8; 8] = b"BLOCFIR1";

/// Si el .exe acaba en su firma (64 bytes r‖s y BLOCFIR1) y cuadra con la clave pública sobre todo lo de delante. Con
/// la criptografía de Windows (CNG), sin nada más.
fn firmado(datos: &[u8]) -> bool {
    type Resumen = unsafe extern "system" fn(usize, *const u8, u32, *const u8, u32, *mut u8, u32) -> i32;
    type Abrir = unsafe extern "system" fn(*mut usize, *const u16, *const u16, u32) -> i32;
    type Importar = unsafe extern "system" fn(usize, usize, *const u16, *mut usize, *const u8, u32, u32) -> i32;
    type Comprobar = unsafe extern "system" fn(usize, *const c_void, *const u8, u32, *const u8, u32, u32) -> i32;
    type Soltar = unsafe extern "system" fn(usize) -> i32;
    type Cerrar = unsafe extern "system" fn(usize, u32) -> i32;
    let Some(n) = datos.len().checked_sub(64 + MARCA_FIRMA.len()).filter(|_| datos.ends_with(MARCA_FIRMA)) else { return false };
    let (cuerpo, firma) = (&datos[..n], &datos[n..n + 64]);
    let f = |nombre| funcion("bcrypt.dll", nombre);
    let (Some(resumen), Some(abrir), Some(importar), Some(comprobar), Some(soltar_clave), Some(cerrar)) =
        (f("BCryptHash"), f("BCryptOpenAlgorithmProvider"), f("BCryptImportKeyPair"), f("BCryptVerifySignature"), f("BCryptDestroyKey"), f("BCryptCloseAlgorithmProvider"))
    else {
        return false;
    };
    let (resumen, abrir, importar, comprobar, soltar_clave, cerrar) = unsafe {
        (
            std::mem::transmute::<usize, Resumen>(resumen),
            std::mem::transmute::<usize, Abrir>(abrir),
            std::mem::transmute::<usize, Importar>(importar),
            std::mem::transmute::<usize, Comprobar>(comprobar),
            std::mem::transmute::<usize, Soltar>(soltar_clave),
            std::mem::transmute::<usize, Cerrar>(cerrar),
        )
    };
    // SHA-256 con BCRYPT_SHA256_ALG_HANDLE; ECDSA con su proveedor abierto (importar una clave no admite el atajo).
    let mut hash = [0u8; 32];
    if unsafe { resumen(0x41, std::ptr::null(), 0, cuerpo.as_ptr(), cuerpo.len() as u32, hash.as_mut_ptr(), 32) } != 0 {
        return false;
    }
    let mut alg = 0usize;
    if unsafe { abrir(&mut alg, ws("ECDSA_P256").as_ptr(), std::ptr::null(), 0) } != 0 {
        return false;
    }
    // BCRYPT_ECCKEY_BLOB: «ECS1» y 32 bytes por coordenada.
    let bloque = [&0x3153_4345u32.to_le_bytes()[..], &32u32.to_le_bytes(), &CLAVE_PUBLICA].concat();
    let mut clave = 0usize;
    let mut bien = unsafe { importar(alg, 0, ws("ECCPUBLICBLOB").as_ptr(), &mut clave, bloque.as_ptr(), bloque.len() as u32, 0) } == 0;
    if bien {
        bien = unsafe { comprobar(clave, std::ptr::null(), hash.as_ptr(), 32, firma.as_ptr(), 64, 0) } == 0;
        unsafe { soltar_clave(clave) };
    }
    unsafe { cerrar(alg, 0) };
    bien
}

/// Lo que recibe la ventana cuando buscar_actualizacion ha puesto una versión nueva, con la versión en wparam (para
/// aviso_actualizacion).
pub const WM_ACTUALIZADO: u32 = WM_APP + 20;
/// Sin nada elegido (puesta al día desde la 1.1): la ventana enseña la página de elegir.
pub const WM_ELEGIR: u32 = WM_APP + 21;

/// La ventana la llama después de su primer pintado. Va en su propio hilo, que nadie espera: si la carpeta de red no
/// responde, abrir, escribir y cerrar siguen igual.
pub fn buscar_actualizacion(ventana: HWND) {
    let v = ventana as usize;
    std::thread::spawn(move || {
        if es_instalado() {
            // Sin buscar las del Bloc de notas (eso es para la página): solo lo guardado.
            match (lista("Abrir"), lista("Vista")) {
                (Some(abrir), Some(vista)) => {
                    let vista: Vec<&str> = vista.iter().map(String::as_str).collect();
                    if let Some(exe) = destino() {
                        // «Editar con Bloc» llegó en la 1.3: la primera vez, marcado (decidido por mí, se puede cambiar).
                        let nuevo = leer(BLOC, "Editar").is_none();
                        if nuevo {
                            anotar(BLOC, Some("Editar"), "1");
                            poner_editar(&exe, true, &abrir.iter().map(String::as_str).collect::<Vec<_>>());
                        }
                        if poner_vistas(&exe, &vista) || nuevo {
                            unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) };
                        }
                    }
                }
                // Sin la entrada de Aplicaciones instaladas, se está desinstalando: no se vuelve a registrar nada.
                _ if leer(DESINSTALAR, "DisplayName").is_some() => unsafe {
                    PostMessageW(v as HWND, WM_ELEGIR, 0, 0);
                },
                _ => {}
            }
        }
        quitar_menu_corto_pendiente();
        crate::informe::mandar_pendientes();
        if let Some(n) = actualizar() {
            unsafe { PostMessageW(v as HWND, WM_ACTUALIZADO, n as usize, 0) };
        }
    });
}

pub fn aviso_actualizacion(wp: WPARAM) -> String {
    con(t().actualizado, texto_version(wp as u64))
}

/// Si en la carpeta de donde se instaló hay un Bloc más nuevo, lo pone en lugar del instalado: las ventanas abiertas
/// siguen con el suyo y las nuevas abren el nuevo. Como mucho una vez por hora entre todas las ventanas.
fn actualizar() -> Option<u64> {
    let destino = destino()?;
    if !es(&std::env::current_exe().ok()?, &destino) {
        return None;
    }
    let origen = PathBuf::from(leer(BLOC, "Origen")?);
    let ahora = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if leer(BLOC, "Comprobado").and_then(|c| c.parse::<u64>().ok()).is_some_and(|c| c <= ahora && ahora - c < 3600) {
        return None;
    }
    anotar(BLOC, Some("Comprobado"), &ahora.to_string());
    let nueva = version_de(&origen)?;
    if nueva <= version_de(&destino)? {
        return None;
    }
    let largo = std::fs::metadata(&origen).ok()?.len();
    // Bloc ocupa unos pocos megas: algo mucho mayor en su sitio no es Bloc, y no se copia.
    if largo > 64 << 20 {
        return None;
    }
    let tmp = copia(&origen, &destino).ok()?;
    // Con la firma de Bloc (la copia, que es lo que se pone); entera y de Bloc; y sin bajar de versión si otra ventana
    // se ha adelantado.
    let firmada = std::fs::read(&tmp).is_ok_and(|d| firmado(&d));
    if !firmada {
        apuntar(&format!("No se pone la versión de {}: no lleva la firma de Bloc", origen.display()));
    }
    let buena = firmada && std::fs::metadata(&tmp).is_ok_and(|m| m.len() == largo) && version_de(&tmp) == Some(nueva) && version_de(&destino).is_some_and(|v| v < nueva);
    if !buena {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    cambiar(&tmp, &destino).ok()?;
    entrada(&destino, Some(nueva));
    Some(nueva)
}

// --- Las ventanas de instalación: el diálogo de tareas de Windows, con sus botones y su barra de progreso ---

// Los botones propios. Las pruebas los pulsan por su número (TDM_CLICK_BUTTON).
const SI: i32 = 100; // Instalar, Actualizar o Desinstalar
const ABRIR: i32 = 102;

struct Pagina {
    titulo: Vec<u16>,
    texto: Option<Vec<u16>>,
    _nombres: Vec<Vec<u16>>,
    botones: Vec<TASKDIALOG_BUTTON>,
    comunes: i32,
}

impl Pagina {
    fn nueva(titulo: &str, texto: Option<&str>, botones: &[(i32, &str)], comunes: i32) -> Pagina {
        let nombres: Vec<Vec<u16>> = botones.iter().map(|b| ws(b.1)).collect();
        let botones = botones.iter().zip(&nombres).map(|(b, n)| TASKDIALOG_BUTTON { nButtonID: b.0, pszButtonText: n.as_ptr() }).collect();
        Pagina { titulo: ws(titulo), texto: texto.map(ws), _nombres: nombres, botones, comunes }
    }

    /// La de en marcha es la misma pregunta, sin texto ni botones y con la barra de progreso.
    fn config(&self, titulo: &[u16], en_marcha: bool) -> TASKDIALOGCONFIG {
        let mut c: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
        c.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
        c.pszWindowTitle = titulo.as_ptr();
        c.pszMainInstruction = self.titulo.as_ptr();
        c.pfCallback = Some(al_dialogo);
        if en_marcha {
            c.dwFlags = TDF_SHOW_MARQUEE_PROGRESS_BAR | TDF_CALLBACK_TIMER;
            c.dwCommonButtons = TDCBF_CANCEL_BUTTON;
        } else {
            c.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION;
            c.dwCommonButtons = self.comunes;
            c.pszContent = self.texto.as_ref().map_or(std::ptr::null(), |t| t.as_ptr());
            c.cButtons = self.botones.len() as u32;
            c.pButtons = self.botones.as_ptr();
        }
        c
    }
}

type Trabajo = Box<dyn FnOnce() -> io::Result<()> + Send>;

struct Estado {
    /// La pregunta, en marcha y, si la hay, la del final.
    paginas: Vec<TASKDIALOGCONFIG>,
    pagina: Cell<usize>,
    trabajo: Cell<Option<Trabajo>>,
    hilo: RefCell<Option<std::thread::JoinHandle<io::Result<()>>>>,
    error: Cell<Option<io::Error>>,
    /// IDCANCEL mientras no se haya hecho nada.
    boton: Cell<i32>,
}

/// Enseña la pregunta y, si se acepta, hace el trabajo en otro hilo (desde la carpeta de red, copiar puede tardar un
/// segundo) con la barra de progreso en marcha, y después la página del final, si la hay. Devuelve el botón con el que
/// se ha cerrado: IDCANCEL si no se ha hecho nada, y SI si se ha hecho y no hay página del final.
fn dialogo(paginas: &[Pagina], trabajo: impl FnOnce() -> io::Result<()> + Send + 'static) -> io::Result<i32> {
    type Dialogo = unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut i32) -> i32;
    // Solo está en comctl32 6, la que pide el manifiesto.
    let f: Dialogo = unsafe { std::mem::transmute(funcion("comctl32.dll", "TaskDialogIndirect").ok_or_else(|| io::Error::other("sin TaskDialogIndirect"))?) };
    let bloc = ws("Bloc");
    let mut configs = vec![paginas[0].config(&bloc, false), paginas[0].config(&bloc, true)];
    configs.extend(paginas.get(1).map(|p| p.config(&bloc, false)));
    let mut e = Estado {
        paginas: configs,
        pagina: Cell::new(0),
        trabajo: Cell::new(Some(Box::new(trabajo))),
        hilo: RefCell::new(None),
        error: Cell::new(None),
        boton: Cell::new(IDCANCEL),
    };
    let datos = &raw mut e;
    for c in unsafe { &mut (*datos).paginas } {
        c.lpCallbackData = datos as isize;
    }
    let r = unsafe { f(&e.paginas[0], std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
    if r < 0 {
        return Err(io::Error::other(format!("TaskDialogIndirect: {r:#x}")));
    }
    match e.error.take() {
        Some(x) => Err(x),
        None => Ok(e.boton.get()),
    }
}

fn ir(h: HWND, e: &Estado, n: usize) {
    e.pagina.set(n);
    enviar(h, TDM_NAVIGATE_PAGE as u32, 0, &e.paginas[n] as *const _ as isize);
}

unsafe extern "system" fn al_dialogo(h: HWND, aviso: i32, wp: WPARAM, _: LPARAM, datos: isize) -> i32 {
    let e = unsafe { &*(datos as *const Estado) };
    match (aviso, e.pagina.get()) {
        // Mientras trabaja no se puede cancelar: una copia a medias se quedaría a medias.
        (TDN_NAVIGATED, 1) => {
            enviar(h, TDM_ENABLE_BUTTON as u32, IDCANCEL as usize, 0);
            enviar(h, TDM_SET_PROGRESS_BAR_MARQUEE as u32, 1, 0);
        }
        (TDN_TIMER, 1) => {
            let acabado = e.hilo.borrow().as_ref().is_some_and(|h| h.is_finished());
            match acabado.then(|| e.hilo.take().map(|h| h.join().unwrap_or_else(|_| Err(io::Error::other("fallo"))))).flatten() {
                Some(Ok(())) if e.paginas.len() > 2 => ir(h, e, 2),
                Some(r) => {
                    e.error.set(r.err());
                    e.boton.set(SI);
                    enviar(h, TDM_ENABLE_BUTTON as u32, IDCANCEL as usize, 1);
                    enviar(h, TDM_CLICK_BUTTON as u32, IDCANCEL as usize, 0);
                }
                None => {}
            }
        }
        (TDN_BUTTON_CLICKED, pagina) => match (pagina, wp as i32) {
            (0, SI) => {
                let trabajo = e.trabajo.take();
                *e.hilo.borrow_mut() = Some(std::thread::spawn(move || trabajo.map_or(Ok(()), |f| f())));
                ir(h, e, 1);
                return S_FALSE;
            }
            (1, _) => return if e.boton.get() == IDCANCEL { S_FALSE } else { S_OK },
            // La X de la página del final cierra como su botón Cerrar.
            (2, IDCANCEL) => e.boton.set(IDCLOSE),
            (_, b) => e.boton.set(b),
        },
        _ => {}
    }
    S_OK
}

// --- Desinstalar ---

/// «Bloc.exe /desinstalar», desde Configuración › Aplicaciones instaladas. La configuración (%APPDATA%\Bloc) se queda,
/// como en casi todos los programas.
pub fn desinstalar() {
    let Some(destino) = destino() else { return };
    let t = t();
    let paginas = [
        Pagina::nueva(t.desinstalar_titulo, Some(t.desinstalar_texto), &[(SI, t.desinstalar_si)], TDCBF_CANCEL_BUTTON),
        Pagina::nueva(t.desinstalado, None, &[], TDCBF_CLOSE_BUTTON),
    ];
    match dialogo(&paginas, || {
        quitar_todo();
        Ok(())
    }) {
        Ok(IDCANCEL) => {}
        Ok(_) => {
            let pruebas = std::env::var_os("BLOC_REGISTRO").is_some();
            // El certificado solo lo quita un administrador: con el aviso de Windows; si se dice que no, se queda.
            if !pruebas && certificado_puesto() {
                unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
                como_administrador(&destino, "/sin-certificado");
            }
            borrar_carpeta(destino.parent().unwrap_or(Path::new(".")), !pruebas && paquete_puesto());
        }
        Err(e) => apuntar(&format!("No se ha podido desinstalar: {e}")),
    }
}

/// Lo que pone registrar, la lista de la barra de tareas y el acceso del menú Inicio.
fn quitar_todo() {
    // Antes de quitar Software\Bloc, donde están las elegidas.
    let exts = conocidas();
    quitar_lista();
    if let Some(exe) = destino() {
        poner_vistas(&exe, &[]);
        poner_editar(&exe, false, &[]);
    }
    quitar_menu_corto();
    quitar_valor(INICIO, "Bloc");
    for c in [r"Applications\Bloc.exe", "Bloc.Texto", "Bloc.Markdown"] {
        quitar(&format!(r"{CLASES}\{c}"));
    }
    quitar(BLOC);
    quitar(DESINSTALAR);
    quitar_valor(r"Software\RegisteredApplications", "Bloc");
    for ext in exts {
        quitar_abrir(&ext);
    }
    if let Some(lnk) = menu_inicio() {
        let _ = std::fs::remove_file(lnk);
    }
    unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) };
}

/// Un .exe abierto no se puede borrar, ni siquiera a sí mismo: la carpeta la borra cmd, que lo intenta cada segundo
/// hasta que se cierran este Bloc y los que sigan abiertos (diez minutos como mucho). Sin pedir administrador.
/// Con `paquete`, después quita el del menú corto: con un Bloc abierto desde él, antes lo habría cerrado.
fn borrar_carpeta(dir: &Path, paquete: bool) {
    let d = dir.display();
    let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into())).join(r"System32\cmd.exe");
    // Con las rutas de System32: cmd buscaría antes en su carpeta de trabajo.
    // El paquete, en cuanto se cierra el último Bloc (quitarlo antes cerraría uno abierto desde él sin preguntar), y
    // también si pasan ocho horas: luego nada lo volvería a intentar.
    let quitar = r#"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -NonInteractive -Command "Get-AppxPackage Bloc.Menu | Remove-AppxPackage""#;
    let (al_borrar, al_final) = if paquete { (format!("{quitar} & exit"), format!(" & {quitar}")) } else { ("exit".to_string(), String::new()) };
    let vueltas = if paquete { 28_800 } else { 600 };
    let orden = format!(r#"/d /c (for /l %i in (1,1,{vueltas}) do @(%SystemRoot%\System32\PING.EXE -n 2 127.0.0.1 >nul & rd /s /q "{d}" 2>nul & if not exist "{d}" ({al_borrar}))){al_final}"#);
    if let Err(e) = Command::new(cmd).raw_arg(orden).current_dir(std::env::temp_dir()).creation_flags(CREATE_NO_WINDOW).spawn() {
        apuntar(&format!("No se ha podido borrar {d}: {e}"));
    }
}

// --- El registro, siempre en HKCU: sin pedir administrador ---

fn clave(ruta: &str) -> Option<HKEY> {
    let r = en_registro(ruta);
    let mut k: HKEY = std::ptr::null_mut();
    let e = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, r.as_ptr(), 0, std::ptr::null(), 0, KEY_WRITE, std::ptr::null(), &mut k, std::ptr::null_mut()) };
    (e == 0).then_some(k)
}

fn poner(k: HKEY, nombre: Option<&str>, valor: &str) {
    let n = nombre.map(ws);
    let v = ws(valor);
    unsafe { RegSetValueExW(k, n.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32) };
}

fn poner_numero(k: HKEY, nombre: &str, valor: u32) {
    let n = ws(nombre);
    unsafe { RegSetValueExW(k, n.as_ptr(), 0, REG_DWORD, &valor as *const u32 as *const u8, 4) };
}

/// Un valor de Software\Bloc, para los demás módulos.
pub fn valor(nombre: &str) -> Option<String> {
    leer(BLOC, nombre)
}

pub fn poner_valor(nombre: &str, valor: &str) {
    anotar(BLOC, Some(nombre), valor);
}

fn anotar(ruta: &str, nombre: Option<&str>, valor: &str) {
    if let Some(k) = clave(ruta) {
        poner(k, nombre, valor);
        unsafe { RegCloseKey(k) };
    }
}

fn leer(ruta: &str, nombre: &str) -> Option<String> {
    leer_en(HKEY_CURRENT_USER, &en_registro(ruta), nombre)
}

/// Un texto del registro (también REG_EXPAND_SZ, tal cual).
fn leer_en(raiz: HKEY, ruta: &[u16], nombre: &str) -> Option<String> {
    let n = ws(nombre);
    let mut b = vec![0u16; 1024];
    let mut largo = (b.len() * 2) as u32;
    let e = unsafe { RegGetValueW(raiz, ruta.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND, std::ptr::null_mut(), b.as_mut_ptr().cast(), &mut largo) };
    (e == 0).then(|| String::from_utf16_lossy(&b[..(largo as usize / 2).saturating_sub(1)]))
}

/// Los tipos de archivo como los ve Windows (HKCR); en las pruebas, solo los de su clave de registro.
fn clases() -> (HKEY, String) {
    match std::env::var("BLOC_REGISTRO") {
        Ok(r) => (HKEY_CURRENT_USER, format!(r"{r}\{CLASES}")),
        Err(_) => (HKEY_CLASSES_ROOT, String::new()),
    }
}

fn de_clase(ruta: &str, nombre: &str) -> Option<String> {
    let (raiz, base) = clases();
    leer_en(raiz, &ws(&if base.is_empty() { ruta.to_string() } else { format!(r"{base}\{ruta}") }), nombre)
}

fn quitar(ruta: &str) {
    let r = en_registro(ruta);
    unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, r.as_ptr()) };
}

fn quitar_valor(ruta: &str, nombre: &str) {
    let (r, n) = (en_registro(ruta), ws(nombre));
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, r.as_ptr(), n.as_ptr()) };
}

fn abierta(ruta: &str) -> Option<HKEY> {
    let r = en_registro(ruta);
    let mut k: HKEY = std::ptr::null_mut();
    (unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, r.as_ptr(), 0, KEY_READ, &mut k) } == 0).then_some(k)
}

fn subclaves(ruta: &str) -> Vec<String> {
    let Some(k) = abierta(ruta) else { return Vec::new() };
    let v = subclaves_de(k, |_| true);
    unsafe { RegCloseKey(k) };
    v
}

/// Las de una clave abierta. Un nombre de más de 255 letras (ERROR_MORE_DATA) se salta: ninguno de los que busca Bloc.
/// Las subclaves hasta la primera que no quiere `seguir` (el registro las da en orden alfabético).
fn subclaves_de(k: HKEY, seguir: impl Fn(&str) -> bool) -> Vec<String> {
    let mut v = Vec::new();
    let mut b = [0u16; 256];
    for i in 0.. {
        let mut n = b.len() as u32;
        match unsafe { RegEnumKeyExW(k, i, b.as_mut_ptr(), &mut n, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) } {
            0 => {
                let s = String::from_utf16_lossy(&b[..n as usize]);
                if !seguir(&s) {
                    break;
                }
                v.push(s);
            }
            234 => {}
            _ => break,
        }
    }
    v
}

/// Los valores de texto de una clave, con su nombre.
fn valores(ruta: &str) -> Vec<(String, String)> {
    let Some(k) = abierta(ruta) else { return Vec::new() };
    let mut v = Vec::new();
    let (mut nombre, mut dato) = (vec![0u16; 1024], vec![0u16; 1024]);
    for i in 0.. {
        let (mut n, mut d, mut tipo) = (nombre.len() as u32, (dato.len() * 2) as u32, 0u32);
        if unsafe { RegEnumValueW(k, i, nombre.as_mut_ptr(), &mut n, std::ptr::null(), &mut tipo, dato.as_mut_ptr().cast(), &mut d) } != 0 {
            break;
        }
        if tipo == REG_SZ {
            v.push((String::from_utf16_lossy(&nombre[..n as usize]), String::from_utf16_lossy(&dato[..(d as usize / 2).saturating_sub(1)])));
        }
    }
    unsafe { RegCloseKey(k) };
    v
}

/// Las claves de los tipos de archivo pueden ser también de otros programas: solo se quitan si quedan vacías.
fn quitar_si_vacia(ruta: &str) {
    let Some(k) = abierta(ruta) else { return };
    let r = en_registro(ruta);
    let (mut claves, mut valores) = (0u32, 0u32);
    let nada = std::ptr::null_mut();
    let e = unsafe { RegQueryInfoKeyW(k, std::ptr::null_mut(), nada, std::ptr::null(), &mut claves, nada, nada, &mut valores, nada, nada, nada, std::ptr::null_mut()) };
    unsafe { RegCloseKey(k) };
    if e == 0 && claves == 0 && valores == 0 {
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, r.as_ptr()) };
    }
}

fn tipo(ext: &str) -> &'static str {
    if matches!(ext, ".md" | ".markdown") { "Bloc.Markdown" } else { "Bloc.Texto" }
}

/// La entrada de Configuración › Aplicaciones instaladas.
fn entrada(exe: &Path, version: Option<u64>) {
    let Some(k) = clave(DESINSTALAR) else { return };
    let e = exe.display();
    poner(k, Some("DisplayName"), "Bloc");
    if let Some(v) = version {
        poner(k, Some("DisplayVersion"), &texto_version(v));
    }
    poner(k, Some("DisplayIcon"), &format!("{e},0"));
    poner(k, Some("UninstallString"), &format!("\"{e}\" /desinstalar"));
    if let Some(d) = exe.parent() {
        poner(k, Some("InstallLocation"), &d.display().to_string());
    }
    poner_numero(k, "EstimatedSize", std::fs::metadata(exe).map_or(0, |m| m.len().div_ceil(1024)) as u32);
    poner_numero(k, "NoModify", 1);
    poner_numero(k, "NoRepair", 1);
    unsafe { RegCloseKey(k) };
}

fn registrar(exe: &Path, origen: &Path) {
    let abrir = format!("\"{}\" \"%1\"", exe.display());
    let icono = format!("{},0", exe.display());
    anotar(&format!(r"{CLASES}\Applications\Bloc.exe"), Some("FriendlyAppName"), "Bloc");
    anotar(&format!(r"{CLASES}\Applications\Bloc.exe\shell\open\command"), None, &abrir);
    // Como aplicación con sus tipos de archivo: así sale en «Aplicaciones predeterminadas», con el botón que la pone
    // para todos de una vez.
    let texto = t().filtro_texto.split(" (").next().unwrap_or("Texto");
    for (tipo, nombre) in [("Bloc.Texto", texto), ("Bloc.Markdown", "Markdown")] {
        anotar(&format!(r"{CLASES}\{tipo}"), None, nombre);
        anotar(&format!(r"{CLASES}\{tipo}\DefaultIcon"), None, &icono);
        anotar(&format!(r"{CLASES}\{tipo}\shell\open\command"), None, &abrir);
    }
    let capacidades = r"Software\Bloc\Capabilities";
    anotar(capacidades, Some("ApplicationName"), "Bloc");
    anotar(capacidades, Some("ApplicationDescription"), t().descripcion);
    anotar(capacidades, Some("ApplicationIcon"), &icono);
    anotar(r"Software\RegisteredApplications", Some("Bloc"), capacidades);
    // Al actualizar, lo elegido; al instalar, lo pone la página de después (eleccion.rs).
    if let (e, true) = eleccion() {
        aplicar(&e);
    }
    // Para buscar ahí las versiones nuevas.
    anotar(BLOC, Some("Origen"), &universal(origen).to_string_lossy());
    entrada(exe, version_de(exe));
    if let Some(lnk) = menu_inicio()
        && !lnk.exists()
    {
        acceso_directo(exe, &lnk);
    }
    unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) }; // que el Explorador vea el «Abrir con» nuevo
}

// --- Lo elegido en la página de después de instalar (eleccion.rs) ---

pub struct Eleccion {
    /// Las filas de la página: las de siempre y las del Bloc de notas de este PC.
    pub todas: Vec<String>,
    pub abrir: Vec<String>,
    pub vista: Vec<String>,
    pub inicio: bool,
    /// «Editar con Bloc» en el menú del botón derecho.
    pub editar: bool,
}

/// Una extensión que Bloc acepta en el registro: corta y sin nada raro.
fn valida(e: &str) -> bool {
    e.len() > 1 && e.len() <= 32 && e.starts_with('.') && e[1..].chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn lista(nombre: &str) -> Option<Vec<String>> {
    Some(leer(BLOC, nombre)?.split(' ').filter(|e| valida(e)).map(str::to_lowercase).collect())
}

/// Las de siempre y las que se hayan elegido alguna vez, para quitarlas todas al desinstalar.
fn conocidas() -> Vec<String> {
    let mut v: Vec<String> = EXTENSIONES.iter().map(|e| e.to_string()).collect();
    for nombre in ["Abrir", "Vista", "Otras"] {
        for e in lista(nombre).unwrap_or_default() {
            if !v.contains(&e) {
                v.push(e);
            }
        }
    }
    v
}

/// Lo elegido y si estaba guardado, con las filas de la página. De entrada (y en las extensiones que salen nuevas), las
/// dos casillas marcadas solo donde hoy abre el Bloc de notas de Windows, Bloc o nada, y siempre en .md y .markdown;
/// en el resto no se le quitan sus archivos a otro programa; el inicio con Windows, marcado.
pub fn eleccion() -> (Eleccion, bool) {
    let mut todas = conocidas();
    for e in de_notas() {
        if !todas.contains(&e) {
            todas.push(e);
        }
    }
    let vistas = lista("Otras").unwrap_or_default();
    let decidida = |e: &String| EXTENSIONES.contains(&e.as_str()) || vistas.contains(e);
    let de_entrada: Vec<String> = todas.iter().filter(|e| matches!(e.as_str(), ".md" | ".markdown") || sin_otro_programa(e)).cloned().collect();
    match (lista("Abrir"), lista("Vista")) {
        (Some(mut abrir), Some(mut vista)) => {
            for e in de_entrada.iter().filter(|e| !decidida(e)) {
                abrir.push(e.clone());
                vista.push(e.clone());
            }
            (Eleccion { todas, abrir, vista, inicio: leer(INICIO, "Bloc").is_some(), editar: leer(BLOC, "Editar").as_deref() != Some("0") }, true)
        }
        _ => (Eleccion { todas, abrir: de_entrada.clone(), vista: de_entrada, inicio: true, editar: true }, false),
    }
}

/// El programa que abre hoy la extensión: el elegido o, si no hay, el de su tipo.
fn programa(ext: &str) -> Option<String> {
    elegido(ext).or_else(|| de_clase(ext, ""))
}

fn sin_otro_programa(ext: &str) -> bool {
    programa(ext).is_none_or(|p| de_notas_o_bloc(&p))
}

/// Un tipo de archivo que abre el Bloc de notas de Windows (o Bloc).
fn de_notas_o_bloc(p: &str) -> bool {
    [r"Applications\Bloc.exe", "Bloc.Texto", "Bloc.Markdown", r"Applications\notepad.exe", "txtfile", "txtfilelegacy"].iter().any(|b| p.eq_ignore_ascii_case(b))
        || de_clase(&format!(r"{p}\Application"), "ApplicationName").is_some_and(|n| n.contains("Microsoft.WindowsNotepad"))
        || de_clase(&format!(r"{p}\shell\open\command"), "").is_some_and(|o| o.to_lowercase().contains("notepad.exe"))
}

/// `f` sobre cada elemento, en cuatro hilos y en orden: cada consulta al registro o a las asociaciones de Windows espera
/// más de lo que trabaja, y la página tiene que salir enseguida.
fn en_hilos<T: Sync, R: Send>(v: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let f = &f;
    std::thread::scope(|s| {
        let hilos: Vec<_> = v.chunks(v.len().div_ceil(4).max(1)).map(|parte| s.spawn(move || parte.iter().map(f).collect::<Vec<R>>())).collect();
        hilos.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    })
}

/// Cada extensión de este PC con su tipo de archivo, en minúsculas: el elegido para abrirla o, si no hay, el de su clase.
/// Solo el registro, sin red.
fn tipos() -> Vec<(String, Option<String>)> {
    let (raiz, base) = clases();
    let mut k: HKEY = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(raiz, ws(&base).as_ptr(), 0, KEY_READ, &mut k) } != 0 {
        return Vec::new();
    }
    // Las claves del registro valen en cualquier hilo; el puntero, no.
    let clave = k as usize;
    let elegidos: std::collections::HashMap<String, String> =
        en_hilos(&subclaves(ELEGIDOS), |e| elegido(e).map(|p| (e.to_lowercase(), p))).into_iter().flatten().collect();
    let exts: Vec<String> = subclaves_de(k, |e| e.starts_with('.') || e < ".")
        .into_iter()
        .map(|e| e.to_lowercase())
        .filter(|e| valida(e))
        .collect();
    let tipos = en_hilos(&exts, |e| elegidos.get(e).cloned().or_else(|| leer_en(clave as HKEY, &ws(e), "")).map(|p| p.to_lowercase()));
    unsafe { RegCloseKey(k) };
    exts.into_iter().zip(tipos).collect()
}

/// Los tipos que cumplen `f`, cada uno mirado una vez.
fn tipos_que(tipos: &[(String, Option<String>)], f: impl Fn(&str) -> bool + Sync) -> std::collections::HashSet<String> {
    let mut distintos: Vec<&String> = tipos.iter().filter_map(|(_, p)| p.as_ref()).collect();
    distintos.sort();
    distintos.dedup();
    distintos.iter().zip(en_hilos(&distintos, |p| f(p))).filter(|(_, si)| *si).map(|(p, _)| p.to_string()).collect()
}

/// Las extensiones que hoy abre el Bloc de notas de Windows en este PC, aparte de las de siempre: por el programa
/// elegido o por la orden de abrir de su tipo.
pub fn de_notas() -> Vec<String> {
    let tipos: Vec<_> = tipos().into_iter().filter(|(e, _)| !EXTENSIONES.contains(&e.as_str())).collect();
    let de_notas = tipos_que(&tipos, de_notas_o_bloc);
    let mut v: Vec<String> = tipos.into_iter().filter(|(_, p)| p.as_ref().is_some_and(|p| de_notas.contains(p))).map(|(e, _)| e).collect();
    v.sort();
    v.dedup();
    // La página tiene sitio para 48 filas: las de siempre y estas.
    v.truncate(48 - EXTENSIONES.len());
    v
}

/// Con qué se abre hoy cada extensión, como lo llama Windows.
pub fn ahora(exts: &[String]) -> Vec<String> {
    type Asociado = unsafe extern "system" fn(u32, u32, *const u16, *const u16, *mut u16, *mut u32) -> i32;
    let f = funcion("shlwapi.dll", "AssocQueryStringW").map(|f| unsafe { std::mem::transmute::<usize, Asociado>(f) });
    en_hilos(exts, |ext| {
        let e = ws(ext);
        let mut b = [0u16; 260];
        let mut n = b.len() as u32;
        // ASSOCF_INIT_IGNOREUNKNOWN y ASSOCSTR_FRIENDLYAPPNAME: sin programa, falla en vez de decir «Abrir con».
        let r = f.map_or(-1, |f| unsafe { f(0x400, 4, e.as_ptr(), std::ptr::null(), b.as_mut_ptr(), &mut n) });
        if r == 0 { String::from_utf16_lossy(&b[..(n as usize).saturating_sub(1)]) } else { t().ningun_programa.to_string() }
    })
}

/// Lo elegido, guardado y puesto: las extensiones en «Abrir con» y en la página de Bloc de Aplicaciones
/// predeterminadas, la vista previa y el inicio con Windows. Lo desmarcado se quita.
pub fn aplicar(e: &Eleccion) {
    let Some(exe) = destino() else { return };
    anotar(BLOC, Some("Abrir"), &e.abrir.join(" "));
    anotar(BLOC, Some("Vista"), &e.vista.join(" "));
    // Las que se han enseñado en la página: las que salgan nuevas después, con su marcado de entrada.
    let otras: Vec<&str> = e.todas.iter().map(String::as_str).filter(|x| !EXTENSIONES.contains(x)).collect();
    anotar(BLOC, Some("Otras"), &otras.join(" "));
    let asociaciones = r"Software\Bloc\Capabilities\FileAssociations";
    for ext in e.todas.iter().map(String::as_str) {
        if e.abrir.iter().any(|x| x == ext) {
            anotar(asociaciones, Some(ext), tipo(ext));
            anotar(&format!(r"{CLASES}\{ext}\OpenWithProgids"), Some(tipo(ext)), "");
            if let Some(k) = clave(&format!(r"{CLASES}\{ext}\OpenWithList\Bloc.exe")) {
                unsafe { RegCloseKey(k) };
            }
        } else {
            quitar_valor(asociaciones, ext);
            quitar_abrir(ext);
        }
    }
    let vista: Vec<&str> = e.vista.iter().map(String::as_str).collect();
    poner_vistas(&exe, &vista);
    anotar(BLOC, Some("Editar"), if e.editar { "1" } else { "0" });
    let abrir: Vec<&str> = e.abrir.iter().map(String::as_str).collect();
    poner_editar(&exe, e.editar, &abrir);
    if e.inicio {
        anotar(INICIO, Some("Bloc"), &format!("\"{}\"", exe.display()));
    } else {
        quitar_valor(INICIO, "Bloc");
    }
    unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) }; // SHCNE_ASSOCCHANGED
}

fn quitar_abrir(ext: &str) {
    let e = format!(r"{CLASES}\{ext}");
    quitar_valor(&format!(r"{e}\OpenWithProgids"), tipo(ext));
    quitar(&format!(r"{e}\OpenWithList\Bloc.exe"));
    for c in [format!(r"{e}\OpenWithProgids"), format!(r"{e}\OpenWithList"), e] {
        quitar_si_vacia(&c);
    }
}

/// Windows no deja que un programa se ponga solo de predeterminado: la página de Bloc en Configuración, con sus
/// extensiones.
pub fn pantalla_predeterminados(h: HWND) {
    let s = ws("ms-settings:defaultapps?registeredAppUser=Bloc");
    unsafe { ShellExecuteW(h, std::ptr::null(), s.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
}

pub fn es_instalado() -> bool {
    matches!((std::env::current_exe(), destino()), (Ok(yo), Some(d)) if es(&yo, &d))
}

/// Si el menú corto lleva «Editar con Bloc» en esa extensión (menu.rs): con su casilla y la de la extensión marcadas.
pub fn en_menu(ext: Option<&str>) -> bool {
    leer(BLOC, "Editar").as_deref() != Some("0") && ext.is_none_or(|e| lista("Abrir").is_some_and(|l| l.iter().any(|x| x == e)))
}

pub fn inicio_con_windows() -> bool {
    leer(INICIO, "Bloc").is_some()
}

// --- La vista previa del Explorador (previa.rs) ---

/// Cada clave donde Bloc ha puesto su vista previa, con el valor que tenía antes (vacío si no había) para devolverlo.
const VISTAS: &str = r"Software\Bloc\VistaPrevia";
const ELEGIDOS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts";
const MANEJADORES: &str = r"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers";

fn texto_guid(g: &GUID) -> String {
    let d = g.data4;
    format!("{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}", g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7])
}

/// El tipo de archivo con el que se ha elegido abrir la extensión: Windows 11 lo apunta en UserChoiceLatest y, antes,
/// en UserChoice.
fn elegido(ext: &str) -> Option<String> {
    leer(&format!(r"{ELEGIDOS}\{ext}\UserChoiceLatest\ProgId"), "ProgId").or_else(|| leer(&format!(r"{ELEGIDOS}\{ext}\UserChoice"), "ProgId"))
}

fn es_mia(v: &str) -> bool {
    [CLSID_TEXTO, CLSID_MD].iter().any(|g| v.eq_ignore_ascii_case(&texto_guid(g)))
}

/// La vista previa de Bloc en las extensiones marcadas, donde gane a la de otros programas (PowerToys pone la suya bajo
/// la extensión): bajo el tipo de archivo elegido para abrirla, si no lo usa ninguna extensión sin marcar, y si no, bajo
/// la extensión. Lo que había se apunta, y lo que Bloc puso y ya no hace falta vuelve a como estaba. Sin ninguna
/// marcada, fuera también sus clases. Solo escribe lo que no está ya, y dice si ha cambiado algo.
fn poner_vistas(exe: &Path, marcadas: &[&str]) -> bool {
    let mut cambiado = false;
    let elegidos: Vec<(String, String)> = subclaves(ELEGIDOS).into_iter().filter_map(|e| elegido(&e).map(|p| (e.to_lowercase(), p))).collect();
    let solo_marcadas = |p: &str| elegidos.iter().all(|(e, q)| !q.eq_ignore_ascii_case(p) || marcadas.contains(&e.as_str()));
    // Cada clave, y si todas las extensiones que la usan son Markdown.
    let mut sitios: Vec<(String, bool)> = Vec::new();
    for ext in marcadas {
        let base = match elegidos.iter().find(|(e, _)| e == ext) {
            Some((_, p)) if solo_marcadas(p) => p.as_str(),
            _ => ext,
        };
        let clave = format!(r"{CLASES}\{base}\shellex\{}", texto_guid(&IID_VISTA));
        let md = tipo(ext) == "Bloc.Markdown";
        match sitios.iter_mut().find(|(c, _)| c.eq_ignore_ascii_case(&clave)) {
            Some(s) => s.1 &= md,
            None => sitios.push((clave, md)),
        }
    }
    for (clave, antes) in valores(VISTAS) {
        if !sitios.iter().any(|(c, _)| c.eq_ignore_ascii_case(&clave)) {
            devolver(&clave, &antes, es_mia);
            quitar_valor(VISTAS, &clave);
            cambiado = true;
        }
    }
    for (clave, md) in &sitios {
        let ahora = leer(clave, "");
        // Lo que había: también si otro programa (PowerToys) ha vuelto a poner la suya después de Bloc.
        match ahora.as_deref() {
            Some(a) if !es_mia(a) => anotar(VISTAS, Some(clave), a),
            None if leer(VISTAS, clave).is_none() => anotar(VISTAS, Some(clave), ""),
            _ => {}
        }
        cambiado |= poner_si(clave, "", &texto_guid(if *md { &CLSID_MD } else { &CLSID_TEXTO }));
    }
    let servidor = format!("\"{}\"", exe.display());
    for (clsid, nombre) in [(CLSID_TEXTO, "Bloc"), (CLSID_MD, "Bloc (Markdown)")] {
        let g = texto_guid(&clsid);
        let c = format!(r"{CLASES}\CLSID\{g}");
        if sitios.is_empty() {
            cambiado |= leer(MANEJADORES, &g).is_some();
            quitar(&c);
            quitar_valor(MANEJADORES, &g);
        } else {
            cambiado |= poner_si(&c, "", nombre) | poner_si(&format!(r"{c}\LocalServer32"), "", &servidor) | poner_si(MANEJADORES, &g, nombre);
        }
    }
    cambiado
}

/// Solo si no está ya así; dice si lo ha cambiado.
fn poner_si(ruta: &str, nombre: &str, valor: &str) -> bool {
    let distinto = !leer(ruta, nombre).is_some_and(|v| v.eq_ignore_ascii_case(valor));
    if distinto {
        anotar(ruta, (!nombre.is_empty()).then_some(nombre), valor);
    }
    distinto
}

/// Si sigue lo de Bloc, lo que había; sin nada antes, fuera la clave y las de encima que queden vacías.
fn devolver(clave: &str, antes: &str, mio: impl Fn(&str) -> bool) {
    if !leer(clave, "").is_some_and(|v| mio(&v)) {
        return;
    }
    if antes.is_empty() {
        quitar_valor(clave, "");
        quitar_vacias(clave);
    } else if let Some(k) = self::clave(clave) {
        // La del Bloc de notas suele ir con %SystemRoot%: se devuelve como estaba, para que Windows la expanda.
        let v = ws(antes);
        let tipo = if con_variables(antes) { REG_EXPAND_SZ } else { REG_SZ };
        unsafe {
            RegSetValueExW(k, std::ptr::null(), 0, tipo, v.as_ptr() as *const u8, (v.len() * 2) as u32);
            RegCloseKey(k);
        }
    }
}

/// Si lleva una variable de entorno (`%SystemRoot%`): el `%1` de las órdenes no lo es.
fn con_variables(s: &str) -> bool {
    let trozos: Vec<&str> = s.split('%').collect();
    trozos.len() > 2 && trozos[1..trozos.len() - 1].iter().any(|v| v.chars().next().is_some_and(char::is_alphabetic) && v.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// La clave y las de encima que se queden vacías, hasta la de los tipos de archivo.
fn quitar_vacias(clave: &str) {
    let mut c = clave;
    while c.len() > CLASES.len() + 1 {
        quitar_si_vacia(c);
        c = c.rsplit_once('\\').map_or("", |x| x.0);
    }
}

// --- «Editar con Bloc» en el menú corto de Windows 11 (menu.rs): el paquete con ubicación externa de Bloc ---

/// Hecho y firmado con privado\paquete\hacer_paquete.py; dice a Windows que «Bloc.exe -Menu» da la orden.
const PAQUETE: &[u8] = include_bytes!("../recursos/Bloc.msix");
/// El certificado público con el que está firmado: Windows solo pone el paquete si se fía de él.
const CERTIFICADO: &[u8] = include_bytes!("../recursos/Bloc.cer");
const FAMILIA: &str = "Bloc.Menu_wzzxxe04mrxg6";
/// «Editar en el Bloc de notas» del menú corto (la orden del paquete del Bloc de notas), que Bloc esconde mientras pone
/// la suya.
const ORDEN_NOTAS: &str = "{CA6CC9F1-867A-481E-951E-A28C5E4F01EA}";
const BLOQUEADAS: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";

fn paquete_puesto() -> bool {
    type Familia = unsafe extern "system" fn(*const u16, *mut u32, *mut *mut u16, *mut u32, *mut u16) -> i32;
    let Some(f) = funcion("kernel32.dll", "GetPackagesByPackageFamily") else { return false };
    let f: Familia = unsafe { std::mem::transmute(f) };
    let (mut n, mut largo) = (0u32, 0u32);
    let r = unsafe { f(ws(FAMILIA).as_ptr(), &mut n, std::ptr::null_mut(), &mut largo, std::ptr::null_mut()) };
    (r == 0 || r == 122) && n > 0 // ERROR_INSUFFICIENT_BUFFER: hay alguno
}

/// Si algún proceso va con la identidad del paquete (un Bloc abierto desde el menú corto): quitar el paquete lo cerraría
/// sin preguntar, con lo que tenga sin guardar.
fn bloc_en_el_paquete() -> bool {
    type Procesos = unsafe extern "system" fn(*mut u32, u32, *mut u32) -> i32;
    type Familia = unsafe extern "system" fn(windows_sys::Win32::Foundation::HANDLE, *mut u32, *mut u16) -> i32;
    let (Some(procesos), Some(familia)) = (funcion("kernel32.dll", "K32EnumProcesses"), funcion("kernel32.dll", "GetPackageFamilyName")) else { return false };
    let (procesos, familia) = unsafe { (std::mem::transmute::<usize, Procesos>(procesos), std::mem::transmute::<usize, Familia>(familia)) };
    let mut ids = vec![0u32; 4096];
    let mut n = 0u32;
    if unsafe { procesos(ids.as_mut_ptr(), (ids.len() * 4) as u32, &mut n) } == 0 {
        return false;
    }
    ids[..n as usize / 4].iter().any(|&id| {
        let h = unsafe { windows_sys::Win32::System::Threading::OpenProcess(0x1000, 0, id) }; // PROCESS_QUERY_LIMITED_INFORMATION
        if h.is_null() {
            return false;
        }
        let mut b = [0u16; 128];
        let mut largo = b.len() as u32;
        let suyo = unsafe { familia(h, &mut largo, b.as_mut_ptr()) } == 0 && String::from_utf16_lossy(&b[..(largo as usize).saturating_sub(1)]) == FAMILIA;
        unsafe { windows_sys::Win32::Foundation::CloseHandle(h) };
        suyo
    })
}

/// Una orden de PowerShell, sin ventana; las rutas van en variables de entorno, nunca dentro de la orden. Dice si ha ido
/// bien.
fn powershell(orden: &str, variables: &[(&str, &std::ffi::OsStr)]) -> bool {
    let ps = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into())).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut c = Command::new(ps);
    c.args(["-NoProfile", "-NonInteractive", "-Command", orden]).creation_flags(CREATE_NO_WINDOW);
    for (n, v) in variables {
        c.env(n, v);
    }
    match c.output() {
        Ok(s) if s.status.success() && s.stderr.is_empty() => true,
        Ok(s) => {
            apuntar(&format!("PowerShell: {}", String::from_utf8_lossy(&s.stderr).trim()));
            false
        }
        Err(e) => {
            apuntar(&format!("PowerShell: {e}"));
            false
        }
    }
}

/// «Personas de confianza» del equipo; `escribir` para poner o quitar (como administrador).
fn con_almacen<R>(escribir: bool, f: impl FnOnce(*mut c_void, &dyn Fn(usize) -> Option<usize>) -> R) -> Option<R> {
    type Abrir = unsafe extern "system" fn(usize, u32, usize, u32, *const u16) -> *mut c_void;
    type Cerrar = unsafe extern "system" fn(*mut c_void, u32) -> i32;
    let cripto = |n: &str| funcion("crypt32.dll", n);
    let (abrir, cerrar) = unsafe { (std::mem::transmute::<usize, Abrir>(cripto("CertOpenStore")?), std::mem::transmute::<usize, Cerrar>(cripto("CertCloseStore")?)) };
    // CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_LOCAL_MACHINE y, para leer, CERT_STORE_READONLY_FLAG.
    let almacen = unsafe { abrir(10, 0, 0, 0x2_0000 | if escribir { 0 } else { 0x8000 }, ws("TrustedPeople").as_ptr()) };
    if almacen.is_null() {
        return None;
    }
    let buscar = |_: usize| -> Option<usize> {
        type Crear = unsafe extern "system" fn(u32, *const u8, u32) -> *mut c_void;
        type Buscar = unsafe extern "system" fn(*mut c_void, u32, u32, u32, *const c_void, *const c_void) -> *mut c_void;
        type Soltar = unsafe extern "system" fn(*mut c_void) -> i32;
        let (crear, buscar, soltar) = unsafe {
            (
                std::mem::transmute::<usize, Crear>(cripto("CertCreateCertificateContext")?),
                std::mem::transmute::<usize, Buscar>(cripto("CertFindCertificateInStore")?),
                std::mem::transmute::<usize, Soltar>(cripto("CertFreeCertificateContext")?),
            )
        };
        let nuestro = unsafe { crear(1, CERTIFICADO.as_ptr(), CERTIFICADO.len() as u32) };
        if nuestro.is_null() {
            return None;
        }
        // X509_ASN_ENCODING, CERT_FIND_EXISTING
        let hallado = unsafe { buscar(almacen, 1, 0, 0x000D_0000, nuestro, std::ptr::null()) };
        unsafe { soltar(nuestro) };
        (!hallado.is_null()).then_some(hallado as usize)
    };
    let r = f(almacen, &buscar);
    unsafe { cerrar(almacen, 0) };
    Some(r)
}

fn certificado_puesto() -> bool {
    con_almacen(false, |_, buscar| {
        let hallado = buscar(0);
        if let (Some(h), Some(soltar)) = (hallado, funcion("crypt32.dll", "CertFreeCertificateContext")) {
            let soltar = unsafe { std::mem::transmute::<usize, unsafe extern "system" fn(usize) -> i32>(soltar) };
            unsafe { soltar(h) };
        }
        hallado.is_some()
    })
    .unwrap_or(false)
}

/// «Bloc.exe /certificado» y «/sin-certificado», que Bloc abre como administrador (con el aviso de Windows): el
/// certificado de Bloc, puesto en o quitado de «Personas de confianza» del equipo. Devuelve el código de salida.
pub fn fiar_certificado(poner: bool) -> i32 {
    type Poner = unsafe extern "system" fn(*mut c_void, u32, *const u8, u32, u32, *mut c_void) -> i32;
    type Quitar = unsafe extern "system" fn(usize) -> i32;
    con_almacen(true, |almacen, buscar| {
        if poner {
            let Some(f) = funcion("crypt32.dll", "CertAddEncodedCertificateToStore") else { return 3 };
            let f = unsafe { std::mem::transmute::<usize, Poner>(f) };
            // X509_ASN_ENCODING, CERT_STORE_ADD_REPLACE_EXISTING
            if unsafe { f(almacen, 1, CERTIFICADO.as_ptr(), CERTIFICADO.len() as u32, 3, std::ptr::null_mut()) } != 0 { 0 } else { 3 }
        } else {
            let (Some(h), Some(f)) = (buscar(0), funcion("crypt32.dll", "CertDeleteCertificateFromStore")) else { return 0 };
            // Borrarlo también suelta el contexto.
            if unsafe { std::mem::transmute::<usize, Quitar>(f)(h) } != 0 { 0 } else { 3 }
        }
    })
    .unwrap_or(2)
}

/// Bloc otra vez, como administrador y con el aviso de Windows. `None` si se ha dicho que no.
fn como_administrador(exe: &Path, orden: &str) -> Option<u32> {
    let w = ws_os(exe.as_os_str());
    let (verbo, args) = (ws("runas"), ws(orden));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verbo.as_ptr();
    info.lpFile = w.as_ptr();
    info.lpParameters = args.as_ptr();
    if unsafe { ShellExecuteExW(&mut info) } == 0 || info.hProcess.is_null() {
        // ERROR_CANCELLED: se ha dicho que no en el aviso. Cualquier otro fallo no lo es.
        return (unsafe { windows_sys::Win32::Foundation::GetLastError() } != 1223).then_some(u32::MAX);
    }
    let mut codigo = 1u32;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut codigo);
        windows_sys::Win32::Foundation::CloseHandle(info.hProcess);
    }
    Some(codigo)
}

/// Después de Aceptar en la página de elegir abierta desde Bloc, en su propio hilo. Con «Editar con Bloc» marcado, el
/// menú corto: si Windows aún no se fía del certificado de Bloc, pide permiso de administrador una vez (el aviso lo
/// contesta el usuario); si se le dice que no, no se vuelve a pedir hasta que se desmarque y se marque otra vez. Sin la
/// casilla, fuera el paquete y vuelve «Editar en el Bloc de notas». En las pruebas (BLOC_REGISTRO) no se toca.
pub fn menu_corto(si: bool) {
    let build = leer_en(HKEY_LOCAL_MACHINE, &ws(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), "CurrentBuild").and_then(|b| b.parse::<u32>().ok());
    if std::env::var_os("BLOC_REGISTRO").is_some() || build.is_none_or(|b| b < 22000) {
        return;
    }
    std::thread::spawn(move || {
        // ShellExecuteEx lo pide.
        unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        if !si {
            quitar_menu_corto();
        } else if paquete_puesto() || (leer(BLOC, "MenuCorto").as_deref() != Some("no") && poner_menu_corto()) {
            unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) };
        }
    });
}

fn poner_menu_corto() -> bool {
    let Some(exe) = destino() else { return false };
    if !certificado_puesto() {
        match como_administrador(&exe, "/certificado") {
            None => {
                anotar(BLOC, Some("MenuCorto"), "no");
                return false;
            }
            Some(0) => {}
            Some(c) => {
                apuntar(&format!("No se ha podido instalar el certificado de Bloc ({c})"));
                return false;
            }
        }
    }
    let msix = std::env::temp_dir().join(format!("Bloc-menu-{}.msix", std::process::id()));
    let carpeta = exe.parent().unwrap_or(Path::new("."));
    let puesto = std::fs::write(&msix, PAQUETE).is_ok()
        && powershell("Add-AppxPackage -Path $env:BLOC_PAQUETE -ExternalLocation $env:BLOC_CARPETA", &[("BLOC_PAQUETE", msix.as_os_str()), ("BLOC_CARPETA", carpeta.as_os_str())]);
    let _ = std::fs::remove_file(&msix);
    puesto
}

/// Con un Bloc abierto desde el menú corto, quitar el paquete lo cerraría: se apunta y se quita al abrir Bloc la
/// próxima vez que no haya ninguno así.
fn quitar_menu_corto() {
    // El que puso una 1.3 de prueba.
    if leer(BLOQUEADAS, ORDEN_NOTAS).as_deref() == Some("Bloc") {
        quitar_valor(BLOQUEADAS, ORDEN_NOTAS);
        quitar_si_vacia(BLOQUEADAS);
    }
    quitar_valor(BLOC, "MenuCorto");
    if std::env::var_os("BLOC_REGISTRO").is_some() || !paquete_puesto() {
        quitar_valor(BLOC, "MenuQuitar");
    } else if bloc_en_el_paquete() {
        anotar(BLOC, Some("MenuQuitar"), "1");
    } else if powershell("Get-AppxPackage Bloc.Menu | Remove-AppxPackage", &[]) {
        quitar_valor(BLOC, "MenuQuitar");
    }
}

/// Al abrir Bloc, en el hilo de buscar actualizaciones: el paquete que no se pudo quitar antes.
fn quitar_menu_corto_pendiente() {
    if leer(BLOC, "MenuQuitar").is_some() && leer(BLOC, "Editar").as_deref() == Some("0") {
        quitar_menu_corto();
    }
}

// --- «Editar con Bloc» en el menú del botón derecho (el clásico, el de «Mostrar más opciones») ---

/// Cada orden de «Editar» que Bloc ha cambiado, con la que tenía antes (vacío si no había) para devolverla.
const EDITAR: &str = r"Software\Bloc\Editar";
const VERBO: &str = r"shell\Bloc.Editar";
const ASOCIADOS: &str = r"Software\Classes\SystemFileAssociations";

fn abre_notas(orden: &str) -> bool {
    let o = orden.to_lowercase();
    let o = o.trim_start_matches('"');
    o.starts_with("notepad.exe") || o.contains(r"\notepad.exe")
}

/// Con `si`, «Editar con Bloc» en los archivos de texto (su tipo percibido) y en las extensiones marcadas, y el
/// «Editar» de los tipos que lo abren con el Bloc de notas, con Bloc. Sin `si`, todo como estaba.
fn poner_editar(exe: &Path, si: bool, exts: &[&str]) {
    let orden = format!("\"{}\" \"%1\"", exe.display());
    let mia = |v: &str| v.eq_ignore_ascii_case(&orden);
    for base in subclaves(ASOCIADOS) {
        let v = format!(r"{ASOCIADOS}\{base}\{VERBO}");
        if leer(&format!(r"{v}\command"), "").is_some() {
            quitar(&v);
            quitar_vacias(&format!(r"{ASOCIADOS}\{base}\shell"));
        }
    }
    let mut sitios: Vec<String> = Vec::new();
    if si {
        for base in std::iter::once("text").chain(exts.iter().copied()) {
            let v = format!(r"{ASOCIADOS}\{base}\{VERBO}");
            anotar(&v, Some("MUIVerb"), t().editar_con);
            anotar(&v, Some("Icon"), &format!("{},0", exe.display()));
            anotar(&format!(r"{v}\command"), None, &orden);
        }
        // Las que ya cambió Bloc siguen; y las que el Bloc de notas tenga ahora.
        sitios = valores(EDITAR).into_iter().map(|(c, _)| c).collect();
        let mut tipos = tipos();
        tipos.push((String::new(), Some("SystemFileAssociations\\text".into())));
        for p in tipos_que(&tipos, |p| de_clase(&format!(r"{p}\shell\edit\command"), "").is_some_and(|o| abre_notas(&o))) {
            let c = format!(r"{CLASES}\{p}\shell\edit\command");
            if !sitios.iter().any(|x| x.eq_ignore_ascii_case(&c)) {
                sitios.push(c);
            }
        }
    }
    for (clave, antes) in valores(EDITAR) {
        if !sitios.iter().any(|c| c.eq_ignore_ascii_case(&clave)) {
            devolver(&clave, &antes, mia);
            quitar_valor(EDITAR, &clave);
        }
    }
    for clave in &sitios {
        if leer(EDITAR, clave).is_none() {
            let antes = leer(clave, "").filter(|v| !mia(v)).unwrap_or_default();
            anotar(EDITAR, Some(clave), &antes);
        }
        poner_si(clave, "", &orden);
    }
}

// --- COM a mano: solo las funciones que hacen falta de cada interfaz, por su número en la tabla ---

const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_C000_000000000046);
const IID_ISHELL_LINK: GUID = GUID::from_u128(0x000214F9_0000_0000_C000_000000000046);
const IID_IPERSIST_FILE: GUID = GUID::from_u128(0x0000010B_0000_0000_C000_000000000046);
const IID_IPROPERTY_STORE: GUID = GUID::from_u128(0x886D8EEB_8CF2_4446_8D02_CDBA1DBDCF99);
const CLSID_DESTINATION_LIST: GUID = GUID::from_u128(0x77F10CF0_3DB5_4966_B520_B7C54FD35ED6);
const IID_ICUSTOM_DESTINATION_LIST: GUID = GUID::from_u128(0x6332DEBF_87B5_4670_90C0_5E57B408A49E);
const CLSID_COLECCION: GUID = GUID::from_u128(0x2D3468C1_36A7_43B6_AC24_D3F02FD9607A);
const IID_IOBJECT_COLLECTION: GUID = GUID::from_u128(0x5632B1A4_E38A_400A_928A_D4CD63230295);
const IID_IOBJECT_ARRAY: GUID = GUID::from_u128(0x92CA9DCD_5622_4BBA_A805_5E9F541BD8C9);

// IShellLinkW: 7 SetDescription, 11 SetArguments, 17 SetIconLocation, 20 SetPath.
fn enlace(exe: &Path, descripcion: &str) -> Option<Com> {
    unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let l = Com::crear(&CLSID_SHELL_LINK, &IID_ISHELL_LINK)?;
    let e = ws_os(exe.as_os_str());
    l.texto(20, &e);
    l.texto(7, &ws(descripcion));
    let icono: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32 = unsafe { std::mem::transmute(l.metodo(17)) };
    unsafe { icono(l.0, e.as_ptr(), 0) };
    Some(l)
}

fn acceso_directo(exe: &Path, lnk: &Path) {
    let Some(l) = enlace(exe, t().descripcion) else { return };
    let Some(f) = l.pedir(&IID_IPERSIST_FILE) else { return };
    let guardar: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32 = unsafe { std::mem::transmute(f.metodo(6)) };
    let r = ws_os(lnk.as_os_str());
    unsafe { guardar(f.0, r.as_ptr(), 1) };
}

#[repr(C)]
struct ClaveDePropiedad {
    fmtid: GUID,
    pid: u32,
}

#[repr(C)]
struct Propiedad {
    vt: u16,
    reservado: [u16; 3],
    valor: *const u16,
    resto: usize,
}

/// «Nueva nota» al pulsar con el botón derecho en Bloc en la barra de tareas. Windows la guarda, así que basta con
/// ponerla al instalar.
pub fn lista_de_tareas() {
    let Ok(exe) = std::env::current_exe() else { return };
    let t = t();
    let hecho = (|| -> Option<()> {
        let tarea = enlace(&exe, t.nueva_nota_ayuda)?;
        let p = tarea.pedir(&IID_IPROPERTY_STORE)?;
        let titulo = ws(t.nueva_nota);
        // PKEY_Title
        let clave = ClaveDePropiedad { fmtid: GUID::from_u128(0xF29F85E0_4FF9_1068_AB91_08002B27B3D9), pid: 2 };
        let valor = Propiedad { vt: 31, reservado: [0; 3], valor: titulo.as_ptr(), resto: 0 }; // VT_LPWSTR
        let poner: unsafe extern "system" fn(*mut c_void, *const ClaveDePropiedad, *const Propiedad) -> i32 = unsafe { std::mem::transmute(p.metodo(6)) };
        let fijar: unsafe extern "system" fn(*mut c_void) -> i32 = unsafe { std::mem::transmute(p.metodo(7)) };
        unsafe {
            if poner(p.0, &clave, &valor) < 0 || fijar(p.0) < 0 {
                return None;
            }
        }
        let lista = Com::crear(&CLSID_DESTINATION_LIST, &IID_ICUSTOM_DESTINATION_LIST)?;
        let coleccion = Com::crear(&CLSID_COLECCION, &IID_IOBJECT_COLLECTION)?;
        let anadir: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32 = unsafe { std::mem::transmute(coleccion.metodo(5)) };
        let empezar: unsafe extern "system" fn(*mut c_void, *mut u32, *const GUID, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(lista.metodo(4)) };
        let tareas: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32 = unsafe { std::mem::transmute(lista.metodo(7)) };
        let terminar: unsafe extern "system" fn(*mut c_void) -> i32 = unsafe { std::mem::transmute(lista.metodo(8)) };
        unsafe {
            let mut huecos = 0u32;
            let mut quitados = std::ptr::null_mut();
            if empezar(lista.0, &mut huecos, &IID_IOBJECT_ARRAY, &mut quitados) < 0 {
                return None;
            }
            if !quitados.is_null() {
                drop(Com(quitados));
            }
            if anadir(coleccion.0, tarea.0) < 0 || tareas(lista.0, coleccion.0) < 0 || terminar(lista.0) < 0 {
                return None;
            }
        }
        Some(())
    })();
    if hecho.is_none() {
        apuntar("No se ha podido poner «Nueva nota» en la barra de tareas");
    }
}

/// Al desinstalar: Windows guarda la lista por programa, y no la quita sola.
fn quitar_lista() {
    unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    if let Some(l) = Com::crear(&CLSID_DESTINATION_LIST, &IID_ICUSTOM_DESTINATION_LIST) {
        // ICustomDestinationList::DeleteList, sin AppID: la de este programa.
        let borrar: unsafe extern "system" fn(*mut c_void, *const u16) -> i32 = unsafe { std::mem::transmute(l.metodo(10)) };
        unsafe { borrar(l.0, std::ptr::null()) };
    }
}

pub fn otra_ventana() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
}

#[cfg(test)]
mod firma {
    const FIRMADO: &[u8] = b"Bloc firma de prueba";
    const FIRMA: [u8; 64] = [
        0xB8, 0xCA, 0x27, 0xE1, 0xC9, 0xCB, 0x79, 0xF6, 0x23, 0x6C, 0x54, 0xAE, 0x3C, 0xFE, 0x93, 0x12, 0x14, 0xE5, 0x21, 0xA7, 0xE9, 0x22, 0x8A, 0x54, 0x0C, 0x8C, 0x79, 0xE8, 0x22, 0x19, 0x5E, 0x6B,
        0x1B, 0x44, 0x54, 0x59, 0x2C, 0xE4, 0x3C, 0x18, 0x76, 0x04, 0xF1, 0x21, 0x3E, 0xBB, 0x72, 0x5F, 0x17, 0x42, 0x5F, 0x09, 0x87, 0x3C, 0x38, 0x7F, 0x41, 0x0C, 0xE6, 0x16, 0xE0, 0x80, 0x58, 0x08,
    ];

    fn con_firma(cuerpo: &[u8]) -> Vec<u8> {
        [cuerpo, &FIRMA, super::MARCA_FIRMA].concat()
    }

    #[test]
    fn bien_firmado() {
        assert!(super::firmado(&con_firma(FIRMADO)));
    }

    #[test]
    fn sin_firma_o_cambiado() {
        assert!(!super::firmado(FIRMADO), "sin firma");
        assert!(!super::firmado(&con_firma(b"Bloc firma de pruebo")), "un byte cambiado");
        let mut otra = con_firma(FIRMADO);
        otra[FIRMADO.len() + 5] ^= 1;
        assert!(!super::firmado(&otra), "la firma cambiada");
        assert!(!super::firmado(b"BLOCFIR1"), "solo la marca");
    }
}
