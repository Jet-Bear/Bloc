// Copia mientras escribes, por si se va la luz o Bloc acaba de golpe. Unos 2 s después de la última tecla (como mucho
// cada 30 s si no se para), el texto de la ventana con cambios va a su copia en la carpeta de Bloc, desde otro hilo y
// con prioridad baja de disco. En cada tecla solo se apunta la hora: el reloj se arma una vez y en reposo no hay
// ninguno. Al abrir Bloc, la copia de una ventana que ya no vive pasa a la nota de la próxima vez.
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::nota::{self, Nota};

pub const RELOJ: usize = 3;
const PAUSA: Duration = Duration::from_secs(2);
const TOPE: Duration = Duration::from_secs(30);

thread_local! {
    /// La primera tecla sin copiar y la última. Sin nada pendiente, no hay reloj.
    static PENDIENTE: Cell<Option<(Instant, Instant)>> = const { Cell::new(None) };
    static HILO: RefCell<Option<JoinHandle<()>>> = const { RefCell::new(None) };
    /// Abierto en exclusiva mientras vive esta ventana: al abrir Bloc, una copia sin el suyo es de una que acabó de golpe.
    static VIVA: RefCell<Option<File>> = const { RefCell::new(None) };
}

fn nombre(ext: &str) -> String {
    format!("copia-{}.{ext}", std::process::id())
}

/// Ha cambiado el texto.
pub fn cambio(h: HWND) {
    let ahora = Instant::now();
    let antes = PENDIENTE.with(|p| p.replace(Some((p.get().map_or(ahora, |(primera, _)| primera), ahora))));
    if antes.is_none() {
        unsafe { SetTimer(h, RELOJ, PAUSA.as_millis() as u32, None) };
    }
}

/// El reloj: si se ha seguido escribiendo, espera a la pausa (sin pasar de 30 s desde la primera tecla); si no, copia lo
/// que dé `nota` (nada si ya no hay cambios).
pub fn reloj(h: HWND, nota: impl FnOnce() -> Option<Nota>) {
    let Some((primera, ultima)) = PENDIENTE.with(Cell::get) else {
        unsafe { KillTimer(h, RELOJ) };
        return;
    };
    let ahora = Instant::now();
    let falta = PAUSA.saturating_sub(ahora - ultima).min(TOPE.saturating_sub(ahora - primera));
    // Una copia que aún se está escribiendo (un disco lento): la siguiente, después.
    let ocupado = HILO.with(|x| x.borrow().as_ref().is_some_and(|j| !j.is_finished()));
    if !falta.is_zero() || ocupado {
        let ms = if ocupado { 200 } else { falta.as_millis().max(1) as u32 };
        unsafe { SetTimer(h, RELOJ, ms, None) };
        return;
    }
    unsafe { KillTimer(h, RELOJ) };
    PENDIENTE.with(|p| p.set(None));
    let Some(n) = nota() else { return };
    VIVA.with(|v| {
        if v.borrow().is_none() {
            *v.borrow_mut() = nota::exclusivo(&nombre("vivo")).ok();
        }
    });
    let hilo = std::thread::spawn(move || {
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
        let _ = nota::escribir_como(&nombre("bloc"), &n);
    });
    HILO.with(|x| *x.borrow_mut() = Some(hilo));
}

/// Lo de esta ventana ya está a salvo (guardado, descartado o en la nota): fuera su copia y su reloj.
pub fn borrar(h: HWND) {
    // La vista previa no tiene copia, y con el número de un proceso de antes se llevaría la de un Bloc que se cerró de golpe.
    if crate::ventana::solo_ver() {
        return;
    }
    if PENDIENTE.with(|p| p.take()).is_some() {
        unsafe { KillTimer(h, RELOJ) };
    }
    if let Some(j) = HILO.with(|x| x.borrow_mut().take()) {
        let _ = j.join();
    }
    nota::borrar_como(&nombre("bloc"));
}

/// Al abrir Bloc: la copia de una ventana que ya no vive (un apagón, un cierre de golpe) pasa a la nota de la próxima
/// vez, o al final de la que haya con la barra; si salió de esa misma nota, la sustituye. Devuelve si ha recuperado
/// alguna.
pub fn recuperar() -> bool {
    let Some(lista) = crate::config::carpeta().and_then(|d| std::fs::read_dir(d).ok()) else { return false };
    let mut hecho = false;
    for e in lista.flatten() {
        let archivo = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = archivo.strip_prefix("copia-").and_then(|r| r.strip_suffix(".bloc")) else { continue };
        let Ok(_vivo) = nota::exclusivo(&format!("copia-{pid}.vivo")) else { continue }; // su ventana sigue abierta
        let Some(copia) = nota::leer_como(&archivo) else { continue };
        let Some(_cerrojo) = nota::cerrojo() else { continue };
        let nueva = match nota::leer() {
            Some(v) if v.serie != copia.serie => nota::anadir(&v, copia),
            Some(v) => Nota { serie: v.serie + 1, ..copia },
            None => Nota { serie: 1, ..copia },
        };
        if nota::escribir(&nueva).is_ok() {
            nota::borrar_como(&archivo);
            hecho = true;
        }
    }
    hecho
}
