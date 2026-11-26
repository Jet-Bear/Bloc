// «Informar de un fallo» (Archivo, y la barra de estado si Bloc falló la vez anterior). Lo escrito y, si se quiere, unos
// datos técnicos que nunca llevan texto de documentos, nombres de archivo ni rutas. Va a la carpeta «Bloc informes» de
// junto a la de donde se instaló Bloc (una carpeta compartida); sin ella, a una incidencia nueva de GitHub que la persona envía con su
// cuenta. Bloc no manda nada por su cuenta. Nada se carga hasta abrir el diálogo, y guardar va en su hilo.
use std::cell::Cell;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, HDC};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::Controls::{CloseThemeData, GetThemePartSize, OpenThemeData, BP_CHECKBOX, CBS_UNCHECKEDNORMAL, TS_TRUE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::dialogo::{color_control, tema_dialogo, texto_ventana, Plantilla, BOTON, EDICION, ESTATICO};
use crate::textos::{limpio, t};
use crate::ventana::{app, poner_mensaje};
use crate::w::{enviar, seguro, ws};

/// El repositorio público de Bloc («usuario/repositorio»). Vacío, el informe se guarda.
const REPOSITORIO: &str = "Jet-Bear/Bloc";
/// Lo que admite el navegador en la dirección de una incidencia nueva, con margen.
const LARGO_DIRECCION: usize = 7000;
/// Cómo acabó el envío, en el wparam: 0 enviado, 1 guardado para mandarlo luego, 2 abierto en el navegador.
pub const WM_INFORMADO: u32 = WM_APP + 22;
const ENVIADO: usize = 0;
const PENDIENTE: usize = 1;
const NAVEGADOR: usize = 2;

const QUE: i32 = 10;
const CAJA: i32 = 11;
const CASILLA: i32 = 12;
const ETIQUETA: i32 = 13;
const VER: i32 = 14;
const SS_NOPREFIX: u32 = 0x80;
const SS_NOTIFY: u32 = 0x100;
/// DWLP_MSGRESULT y DLGPROC van delante, 8 bytes cada uno.
const DWLP_USER: i32 = 16;

thread_local!(static AVISADO: Cell<bool> = const { Cell::new(false) });
// Lo escrito y cerrado sin enviar (Esc, la X o Cancelar): vuelve al abrir el diálogo otra vez.
thread_local!(static BORRADOR: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) });

/// Si la barra de estado dice que Bloc falló la vez anterior: un clic en ella abre el diálogo.
pub fn avisado() -> bool {
    AVISADO.with(Cell::get)
}

/// La barra de estado ya dice otra cosa: un clic en ella no abre el diálogo.
pub fn olvidar_aviso() {
    AVISADO.with(|a| a.set(false));
}

fn registro_errores() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| Path::new(&d).join(r"Bloc\errores.log"))
}

/// Lo que ha apuntado la vista previa (integridad baja), al final del registro de Bloc. Se aparta antes con otro nombre:
/// lo que apunte la vista mientras tanto va a uno nuevo y se junta la próxima vez.
fn juntar_registro_bajo(ruta: &Path) {
    let Some(bajo) = crate::w::registro_bajo() else { return };
    let tomado = bajo.with_extension("log.tomado");
    if std::fs::rename(&bajo, &tomado).is_err() && !tomado.exists() {
        return;
    }
    let Ok(lo_suyo) = std::fs::read(&tomado) else { return };
    if let Some(dir) = ruta.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::OpenOptions::new().create(true).append(true).open(ruta).is_ok_and(|mut f| f.write_all(&lo_suyo).is_ok()) {
        let _ = std::fs::remove_file(&tomado);
    }
}

/// Al abrir Bloc: si desde la última vez se ha apuntado un fallo de Bloc (un pánico), la barra de estado lo dice una vez.
/// Solo lee lo nuevo del registro.
pub fn al_abrir() {
    let Some(ruta) = registro_errores() else { return };
    juntar_registro_bajo(&ruta);
    let Ok(mut f) = std::fs::File::open(&ruta) else { return };
    let largo = f.metadata().map_or(0, |m| m.len());
    // La primera vez (sin lo visto apuntado), los fallos de antes no cuentan.
    let Some(visto) = crate::instalar::valor("ErroresVistos").and_then(|v| v.parse::<u64>().ok()) else {
        crate::instalar::poner_valor("ErroresVistos", &largo.to_string());
        return;
    };
    if largo == visto {
        return;
    }
    crate::instalar::poner_valor("ErroresVistos", &largo.to_string());
    let desde = if visto < largo { visto } else { 0 };
    let mut nuevo = Vec::new();
    if f.seek(SeekFrom::Start(desde)).is_ok() && f.take(1 << 20).read_to_end(&mut nuevo).is_ok() && String::from_utf8_lossy(&nuevo).contains("panicked at") {
        AVISADO.with(|a| a.set(true));
        poner_mensaje(t().informar_fallo_antes);
    }
}

struct Datos {
    informe: Option<String>,
}

/// El diálogo. Con Enviar, se cierra al momento y el informe va en su hilo; al acabar, la barra de estado lo dice.
pub fn preguntar() {
    AVISADO.with(|a| a.set(false));
    let t = t();
    let mut p = Plantilla::nueva(&limpio(t.informar_menu), 280, 142, 0);
    p.control(ESTATICO, t.informar_que, QUE, SS_NOPREFIX, 7, 7, 266, 10);
    p.control(EDICION, "", CAJA, WS_TABSTOP | WS_BORDER | WS_VSCROLL | ES_MULTILINE as u32 | ES_WANTRETURN as u32 | ES_AUTOVSCROLL as u32, 7, 19, 266, 82);
    // La casilla, solo el cuadro: con su texto, Windows lo pinta negro en modo noche (como en la página de elegir).
    p.control(BOTON, t.informar_datos, CASILLA, WS_TABSTOP | BS_AUTOCHECKBOX as u32, 7, 106, 10, 10);
    p.control(ESTATICO, t.informar_datos, ETIQUETA, SS_NOPREFIX | SS_NOTIFY, 19, 107, 160, 10);
    p.control(BOTON, t.informar_ver, VER, WS_TABSTOP | BS_PUSHBUTTON as u32, 7, 121, 110, 14);
    p.control(BOTON, t.informar_enviar, IDOK, WS_TABSTOP | BS_DEFPUSHBUTTON as u32, 158, 121, 55, 14);
    p.control(BOTON, t.cancelar, IDCANCEL, WS_TABSTOP | BS_PUSHBUTTON as u32, 218, 121, 55, 14);
    let plantilla = p.hecha();
    let mut datos = Datos { informe: None };
    unsafe {
        DialogBoxIndirectParamW(GetModuleHandleW(std::ptr::null()), plantilla.as_ptr() as *const DLGTEMPLATE, app().hwnd.get(), Some(proc_informe), &mut datos as *mut Datos as isize);
    }
    if let Some(informe) = datos.informe {
        // Primero en el perfil, al momento: si Bloc se cierra antes de que conteste la carpeta de red, no se pierde.
        let nombre = nombre_informe();
        let guardado = pendientes().and_then(|p| escribir(&p, &nombre, &informe).ok());
        let ventana = app().hwnd.get() as usize;
        std::thread::spawn(move || {
            let r = mandar(&nombre, &informe, guardado.as_deref());
            unsafe { PostMessageW(ventana as HWND, WM_INFORMADO, r, 0) };
        });
    }
}

pub fn aviso(r: WPARAM) -> &'static str {
    match r {
        ENVIADO => t().informar_enviado,
        NAVEGADOR => t().informar_navegador,
        _ => t().informar_pendiente,
    }
}

unsafe extern "system" fn proc_informe(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    seguro(|| mensaje(h, m, wp, lp), || 0)
}

fn mensaje(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> isize {
    let noche = app().cfg.borrow().noche;
    match m {
        WM_INITDIALOG => {
            unsafe { SetWindowLongPtrW(h, DWLP_USER, lp) };
            let casilla = unsafe { GetDlgItem(h, CASILLA) };
            enviar(casilla, BM_SETCHECK, 1, 0);
            ancho_de_cuadro(h, casilla);
            tema_dialogo(h);
            let si: i32 = 1;
            unsafe { DwmSetWindowAttribute(h, 3, &si as *const i32 as *const _, 4) }; // DWMWA_TRANSITIONS_FORCEDISABLED
            BORRADOR.with(|b| crate::dialogo::poner_texto(unsafe { GetDlgItem(h, CAJA) }, &b.borrow()));
            unsafe { SetFocus(GetDlgItem(h, CAJA)) };
            0
        }
        WM_COMMAND => {
            let (id, aviso) = ((wp & 0xFFFF) as i32, (wp >> 16) as u32);
            let marcada = || enviar(unsafe { GetDlgItem(h, CASILLA) }, BM_GETCHECK, 0, 0) == 1;
            let escrito = || String::from_utf16_lossy(&texto_ventana(unsafe { GetDlgItem(h, CAJA) }));
            match (id, aviso) {
                (ETIQUETA, STN_CLICKED) => {
                    enviar(unsafe { GetDlgItem(h, CASILLA) }, BM_SETCHECK, !marcada() as usize, 0);
                }
                (VER, BN_CLICKED) => ver(h, &informe(&escrito(), marcada())),
                (IDOK, BN_CLICKED) => {
                    let d = unsafe { &mut *(GetWindowLongPtrW(h, DWLP_USER) as *mut Datos) };
                    d.informe = Some(informe(&escrito(), marcada()));
                    BORRADOR.with(|b| b.borrow_mut().clear());
                    unsafe { EndDialog(h, IDOK as isize) };
                }
                (IDCANCEL, _) => {
                    BORRADOR.with(|b| *b.borrow_mut() = texto_ventana(unsafe { GetDlgItem(h, CAJA) }));
                    unsafe { EndDialog(h, IDCANCEL as isize) };
                }
                _ => {}
            }
            1
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT if noche => color_control(m, wp as HDC),
        _ => 0,
    }
}

/// La casilla, del ancho justo de su cuadro: más ancha, asoma el principio de su texto.
fn ancho_de_cuadro(h: HWND, casilla: HWND) {
    let mut caja = SIZE { cx: 0, cy: 0 };
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        let tema = OpenThemeData(h, ws("Button").as_ptr());
        if tema != 0 {
            let dc = GetDC(h);
            GetThemePartSize(tema, dc, BP_CHECKBOX, CBS_UNCHECKEDNORMAL, std::ptr::null(), TS_TRUE, &mut caja);
            ReleaseDC(h, dc);
            CloseThemeData(tema);
        }
        GetClientRect(casilla, &mut r);
        if caja.cx > 0 {
            SetWindowPos(casilla, std::ptr::null_mut(), 0, 0, caja.cx, r.bottom, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

/// «Ver qué se envía»: el informe tal cual, para leer.
fn ver(padre: HWND, informe: &str) {
    let mut p = Plantilla::nueva(&limpio(t().informar_ver), 340, 230, 0);
    p.control(EDICION, &informe.replace('\n', "\r\n"), CAJA, WS_TABSTOP | WS_BORDER | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32, 7, 7, 326, 196);
    p.control(BOTON, t().cerrar, IDCANCEL, WS_TABSTOP | BS_DEFPUSHBUTTON as u32, 278, 209, 55, 14);
    let plantilla = p.hecha();
    unsafe { DialogBoxIndirectParamW(GetModuleHandleW(std::ptr::null()), plantilla.as_ptr() as *const DLGTEMPLATE, padre, Some(proc_ver), 0) };
}

unsafe extern "system" fn proc_ver(h: HWND, m: u32, wp: WPARAM, _: LPARAM) -> isize {
    seguro(
        || match m {
            WM_INITDIALOG => {
                tema_dialogo(h);
                1
            }
            WM_COMMAND if (wp & 0xFFFF) as i32 == IDCANCEL => {
                unsafe { EndDialog(h, 0) };
                1
            }
            WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT if app().cfg.borrow().noche => color_control(m, wp as HDC),
            _ => 0,
        },
        || 0,
    )
}

// --- Lo que se envía ---

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// El informe en Markdown: lo escrito y, si se quiere, los datos técnicos.
fn informe(escrito: &str, con_datos: bool) -> String {
    let mut s = format!("# Informe de Bloc {}\n\n{}\n", version(), escrito.replace("\r\n", "\n").trim());
    if con_datos {
        s += "\n## Datos técnicos\n\n";
        s += &datos_tecnicos();
    }
    s
}

fn datos_tecnicos() -> String {
    let a = app();
    let cfg = a.cfg.borrow();
    let si_no = |b: bool| if b { "sí" } else { "no" };
    let texto = crate::ventana::texto();
    let tablas = texto.split(|&c| c == 13).filter(|l| es_separador(&String::from_utf16_lossy(l))).count();
    let kb = texto.len().div_ceil(1024);
    let mut s = format!(
        "- Bloc {} · Windows {} · idioma {:04X} · escala {} % · zoom {} %\n- Noche: {} · Marcas: {} · Ajuste de línea: {}\n- Documento: {}, {} · {} tablas\n",
        version(),
        version_windows(),
        unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() },
        crate::barras::dpi() * 100 / 96,
        cfg.zoom,
        si_no(cfg.noche),
        si_no(cfg.marcas),
        si_no(cfg.ajuste),
        if a.md.get() { ".md" } else { ".txt" },
        redondeado(kb),
        tablas,
    );
    s += &format!("- Opciones: {}\n", opciones());
    let registro = ultimas_lineas(50);
    if !registro.is_empty() {
        s += "\n## Últimas líneas del registro\n\n```text\n";
        s += &registro;
        s += "\n```\n";
    }
    s
}

/// Una línea de separación de tabla de Markdown (`|---|:--:|`).
fn es_separador(l: &str) -> bool {
    let l = l.trim();
    l.starts_with('|') && l.contains('-') && l.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// El tamaño sin precisión de más: no se puede reconocer un documento por él.
fn redondeado(kb: usize) -> String {
    match kb {
        0..=1 => "menos de 2 KB".to_string(),
        2..=99 => format!("unos {} KB", kb.div_ceil(10) * 10),
        _ => format!("unos {} KB", kb.div_ceil(100) * 100),
    }
}

fn version_windows() -> String {
    let clave = ws(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let texto = |n: &str| {
        let mut b = [0u16; 64];
        let mut largo = (b.len() * 2) as u32;
        let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, clave.as_ptr(), ws(n).as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), b.as_mut_ptr().cast(), &mut largo) };
        if r == 0 { String::from_utf16_lossy(&b[..(largo as usize / 2).saturating_sub(1)]) } else { String::new() }
    };
    let mut ubr = 0u32;
    let mut largo = 4u32;
    unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, clave.as_ptr(), ws("UBR").as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut ubr as *mut u32).cast(), &mut largo) };
    format!("{} (compilación {}.{ubr})", texto("DisplayVersion"), texto("CurrentBuild"))
}

/// Las opciones de bloc.ini que dicen cómo se ve y se usa Bloc: ni Recientes, ni la posición, ni nada con rutas.
fn opciones() -> String {
    const VALEN: [&str; 12] = ["ajuste", "barra", "max", "zoom", "noche", "corrector", "marcas", "nuevos_md", "copia", "margenes", "letra", "tamano"];
    let Some(ruta) = crate::config::ruta() else { return String::new() };
    let Ok(s) = std::fs::read_to_string(ruta) else { return String::new() };
    s.lines().filter(|l| l.split_once('=').is_some_and(|(k, _)| VALEN.contains(&k.trim()))).map(str::trim).collect::<Vec<_>>().join(" ")
}

fn ultimas_lineas(n: usize) -> String {
    // Solo el final: el registro no tiene tope.
    let Some(ruta) = registro_errores() else { return String::new() };
    juntar_registro_bajo(&ruta);
    let Ok(mut f) = std::fs::File::open(&ruta) else { return String::new() };
    let largo = f.metadata().map_or(0, |m| m.len());
    let mut s = Vec::new();
    if f.seek(SeekFrom::Start(largo.saturating_sub(64 << 10))).is_err() || f.read_to_end(&mut s).is_err() {
        return String::new();
    }
    let s = String::from_utf8_lossy(&s);
    let mut lineas: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    // La primera, si se ha empezado a medias, fuera.
    if largo > 64 << 10 && !lineas.is_empty() {
        lineas.remove(0);
    }
    lineas[lineas.len().saturating_sub(n)..].iter().map(|l| limpiar(l)).collect::<Vec<_>>().join("\n")
}

/// Una línea del registro sin rutas ni nombres de archivo: cada uno pasa a `<archivo>` con su extensión. Una ruta va
/// desde su unidad (`C:\`, `C:/`) o su servidor (`\\`) hasta unas comillas, «: », el final de la línea o, después de su
/// última barra, el primer espacio tras el punto de la extensión. Las rutas de Bloc en los fallos (`src\…rs:10:5`) se
/// quedan: no tienen unidad, y su «extensión» lleva números. Antes, lo que va entre comillas (nombres de archivo, el
/// trozo de texto que cita un fallo de Rust) pasa a «…».
pub fn limpiar(linea: &str) -> String {
    let c = sin_citas(linea);
    let corte = |x: char| matches!(x, '«' | '»' | '"' | '\'' | '<' | '>' | '|' | '“' | '”');
    let mut s = String::new();
    let mut i = 0;
    while i < c.len() {
        let unidad = i + 2 < c.len() && c[i].is_ascii_alphabetic() && c[i + 1] == ':' && matches!(c[i + 2], '\\' | '/') && (i == 0 || !c[i - 1].is_alphanumeric());
        let servidor = i + 1 < c.len() && c[i] == '\\' && c[i + 1] == '\\';
        if unidad || servidor {
            // Hasta el primer «.ext» que no siga con más nombre, o hasta un corte.
            let mut j = i;
            while j < c.len() && !corte(c[j]) && !(c[j] == ':' && j > i + 1 && c.get(j + 1) == Some(&' ')) {
                // Una carpeta con punto («Sr.Lopez Ruiz\») no acaba la ruta: solo después de su última barra.
                let barra_detras = || {
                    for k in j + 1..c.len() {
                        let otra = k + 2 < c.len() && c[k].is_ascii_alphabetic() && c[k + 1] == ':' && matches!(c[k + 2], '\\' | '/') || c[k] == '\\' && c.get(k + 1) == Some(&'\\');
                        if corte(c[k]) || otra {
                            return false;
                        }
                        if matches!(c[k], '\\' | '/') {
                            return true;
                        }
                    }
                    false
                };
                if c[j] == '.' && !barra_detras() {
                    let e = c[j + 1..].iter().take_while(|x| x.is_ascii_alphanumeric()).count();
                    let acaba = c.get(j + 1 + e).is_none_or(|x| !x.is_alphanumeric() && !matches!(x, '\\' | '/' | '.' | '_' | '-'));
                    if (1..=5).contains(&e) && c[j + 1..j + 1 + e].iter().any(char::is_ascii_alphabetic) && acaba {
                        j += 1 + e;
                        break;
                    }
                }
                j += 1;
            }
            let ultima = c[i..j].iter().rposition(|&x| matches!(x, '\\' | '/')).map_or(i, |k| i + k + 1);
            s += "<archivo>";
            s += &extension(&c[ultima..j].iter().collect::<String>());
            i = j;
        } else if c[i].is_alphanumeric() && (i == 0 || !(c[i - 1].is_alphanumeric() || matches!(c[i - 1], '.' | '_' | '-'))) {
            let mut j = i;
            while j < c.len() && (c[j].is_alphanumeric() || matches!(c[j], '.' | '_' | '-')) {
                j += 1;
            }
            let palabra: String = c[i..j].iter().collect();
            let ext = extension(&palabra);
            let raiz = &palabra[..palabra.len() - ext.len()];
            let de_bloc = s.ends_with("src\\") || s.ends_with("src/");
            if !ext.is_empty() && raiz.chars().any(char::is_alphabetic) && !raiz.ends_with('.') && !de_bloc {
                s += "<archivo>";
                s += &ext;
            } else {
                s += &palabra;
            }
            i = j;
        } else {
            s.push(c[i]);
            i += 1;
        }
    }
    s
}

/// La línea con lo que va entre comillas cambiado por «…»: «», “”, "", '' y ``.
fn sin_citas(linea: &str) -> Vec<char> {
    let c: Vec<char> = linea.chars().collect();
    let mut r = Vec::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        let cierre = match c[i] {
            '«' => Some('»'),
            '“' => Some('”'),
            x @ ('"' | '\'' | '`') => Some(x),
            _ => None,
        };
        if let Some(k) = cierre.and_then(|z| c[i + 1..].iter().position(|&x| x == z)) {
            r.extend("«…»".chars());
            i += k + 2;
        } else {
            r.push(c[i]);
            i += 1;
        }
    }
    r
}

/// «.ext» si el nombre acaba en una extensión de 1 a 5 letras o cifras con alguna letra; si no, nada.
fn extension(nombre: &str) -> String {
    match nombre.trim_end().rsplit_once('.') {
        Some((raiz, e)) if !raiz.is_empty() && (1..=5).contains(&e.len()) && e.chars().all(|x| x.is_ascii_alphanumeric()) && e.chars().any(|x| x.is_ascii_alphabetic()) => {
            format!(".{e}")
        }
        _ => String::new(),
    }
}

// --- Adónde va ---

/// La carpeta «Bloc informes» de junto a la de donde se instaló Bloc; `Err(true)` si esa carpeta de origen no responde.
fn carpeta_compartida() -> Result<PathBuf, bool> {
    let origen = crate::instalar::valor("Origen").map(PathBuf::from).ok_or(false)?;
    let bloc = origen.parent().ok_or(false)?;
    let informes = bloc.parent().ok_or(false)?.join("Bloc informes");
    if informes.is_dir() {
        Ok(informes)
    } else {
        Err(!bloc.is_dir())
    }
}

fn pendientes() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| Path::new(&d).join(r"Bloc\informes"))
}

fn nombre_informe() -> String {
    let mut h = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut h) };
    let usuario: String = std::env::var("USERNAME").unwrap_or_default().chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_')).collect();
    format!("{}-{:02}-{:02} {:02}-{:02} {} {}", h.wYear, h.wMonth, h.wDay, h.wHour, h.wMinute, usuario, version())
}

/// Por temporal y cambio de nombre, sin pisar otro informe.
fn escribir(carpeta: &Path, nombre: &str, informe: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(carpeta)?;
    let mut destino = carpeta.join(format!("{nombre}.md"));
    for n in 2.. {
        if !destino.exists() {
            break;
        }
        destino = carpeta.join(format!("{nombre} ({n}).md"));
    }
    let tmp = destino.with_extension("md.tmp");
    std::fs::write(&tmp, informe)?;
    std::fs::rename(&tmp, &destino).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(destino)
}

/// `guardado`: la copia del perfil, que se borra cuando ha llegado a su sitio.
fn mandar(nombre: &str, informe: &str, guardado: Option<&Path>) -> usize {
    let r = match carpeta_compartida() {
        Ok(c) if escribir(&c, nombre, informe).is_ok() => ENVIADO,
        Err(false) => {
            let repositorio = std::env::var("BLOC_REPOSITORIO").unwrap_or_else(|_| REPOSITORIO.to_string());
            if !repositorio.is_empty() && abrir_incidencia(&repositorio, informe) { NAVEGADOR } else { PENDIENTE }
        }
        _ => PENDIENTE,
    };
    if r != PENDIENTE
        && let Some(g) = guardado
    {
        let _ = std::fs::remove_file(g);
    }
    r
}

/// La página de incidencia nueva con el título y el texto puestos, recortados a lo que admite la dirección. Las pruebas
/// ponen BLOC_INFORME_URL para recibir la dirección en un archivo en vez de abrir el navegador.
fn abrir_incidencia(repositorio: &str, informe: &str) -> bool {
    let titulo = informe.lines().nth(2).unwrap_or_default().chars().take(80).collect::<String>();
    let base = format!("https://github.com/{repositorio}/issues/new?title={}&body=", codificar(&titulo));
    let mut cuerpo = codificar(informe);
    if base.len() + cuerpo.len() > LARGO_DIRECCION {
        // Sin partir un %XX ni una letra de varios bytes (los %8X-%BX siguen a la suya).
        let mut corte = LARGO_DIRECCION - base.len();
        if let Some(k) = cuerpo[..corte].rfind('%').filter(|&k| corte - k < 3) {
            corte = k;
        }
        while corte >= 3 && cuerpo.as_bytes().get(corte) == Some(&b'%') && matches!(cuerpo.as_bytes()[corte + 1], b'8' | b'9' | b'A' | b'B') {
            corte -= 3;
        }
        cuerpo.truncate(corte);
    }
    let direccion = base + &cuerpo;
    if let Some(f) = std::env::var_os("BLOC_INFORME_URL") {
        return std::fs::write(f, &direccion).is_ok();
    }
    // ShellExecute pide COM en su hilo.
    unsafe { windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32) };
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), ws("open").as_ptr(), ws(&direccion).as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    r as isize > 32
}

fn codificar(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Al abrir el Bloc instalado, en su hilo de buscar actualizaciones: los informes que se guardaron sin red, a su carpeta.
pub fn mandar_pendientes() {
    let Some(p) = pendientes() else { return };
    let Ok(leidos) = std::fs::read_dir(&p) else { return };
    // Los «.md» y los que apartó otro Bloc que se cerró a medias (hace más de diez minutos).
    let viejo = |r: &Path| std::fs::metadata(r).and_then(|m| m.modified()).is_ok_and(|m| m.elapsed().is_ok_and(|d| d.as_secs() > 600));
    let mut informes = leidos
        .flatten()
        .map(|e| e.path())
        .filter(|r| {
            let n = r.file_name().map_or(String::new(), |n| n.to_string_lossy().to_string());
            n.ends_with(".md") || n.rsplit_once(".md.").is_some_and(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) && viejo(r)
        })
        .peekable();
    if informes.peek().is_none() {
        return;
    }
    let Ok(compartida) = carpeta_compartida() else { return };
    for r in informes {
        // Cada uno, apartado antes de mandarlo: otra ventana abierta a la vez no lo manda otra vez.
        let n = r.file_name().map_or(String::new(), |n| n.to_string_lossy().to_string());
        let Some((nombre, _)) = n.rsplit_once(".md") else { continue };
        let nombre = nombre.to_string();
        let tomado = r.with_file_name(format!("{nombre}.md.{}", std::process::id()));
        if std::fs::rename(&r, &tomado).is_err() {
            continue;
        }
        match std::fs::read_to_string(&tomado) {
            Ok(s) if escribir(&compartida, &nombre, &s).is_ok() => {
                let _ = std::fs::remove_file(&tomado);
            }
            _ => {
                let _ = std::fs::rename(&tomado, r.with_file_name(format!("{nombre}.md")));
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn sin_rutas_ni_nombres() {
        let l = limpiar(r"24/11/2026 10:00:00  No se pone la versión de D:\Equipo\Programas\Bloc\Bloc.exe: no lleva la firma de Bloc");
        assert_eq!(l, "24/11/2026 10:00:00  No se pone la versión de <archivo>.exe: no lleva la firma de Bloc");
        let l = limpiar(r"Vista previa: el flujo da «Factura Juan Pérez 2026.pdf»; por el Explorador, la ruta «G:\CLIENTES\JUAN PEREZ SL\2026\Factura Juan Pérez 2026.pdf»");
        assert!(!l.to_lowercase().contains("rez") && !l.contains("CLIENTES") && !l.contains("G:"), "{l}");
        let l = limpiar(r"No se abre C:\Clientes\Sr.Lopez Ruiz\Expte.Juan Garcia\carta.docx hoy");
        assert_eq!(l, "No se abre <archivo>.docx hoy");
        let l = limpiar("panicked at src\\html.rs:40:9: byte index 3 is not a char boundary; it is inside 'é' (bytes 2..4) of `Juan García`");
        assert_eq!(l, "panicked at src\\html.rs:40:9: byte index 3 is not a char boundary; it is inside «…» (bytes 2..4) of «…»");
        let l = limpiar(r"No se ha podido guardar \\NAS\datos\notas cliente.txt y carta.docx ni C:/Users/ana/a.md");
        assert_eq!(l, "No se ha podido guardar <archivo>.txt y <archivo>.docx ni <archivo>.md");
    }

    #[test]
    fn deja_lo_de_bloc() {
        let l = "panicked at src\\ventana.rs:123:45: index out of bounds 1.3.0";
        assert_eq!(limpiar(l), l);
    }

    #[test]
    fn separadores_y_tamanos() {
        assert!(es_separador("| --- | :-: |") && !es_separador("| a | b |") && !es_separador("---"));
        assert_eq!(redondeado(903), "unos 1000 KB");
        assert_eq!(redondeado(47), "unos 50 KB");
    }

    #[test]
    fn direccion_bien_formada() {
        assert_eq!(codificar("a b/ñ"), "a%20b%2F%C3%B1");
    }
}
