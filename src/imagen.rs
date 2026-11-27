// M5 (1.4): una imagen pegada en un .md se guarda como PNG junto a él («<nombre> AAAA-MM-DD HH-MM-SS.png») y se enlaza.
// Sin subcarpetas. Lo que no viene ya en PNG lo convierte GDI+ de Windows, que se carga solo al pegar una imagen o al
// dibujar una en un .md (M6), nunca al arrancar.
use std::path::Path;

use windows_sys::Win32::Foundation::{FreeLibrary, GetLastError};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, HALFTONE, SRCCOPY,
};
use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::w::{ws, ws_os};

/// Lo que trae el portapapeles: un PNG tal cual o un mapa de bits de Windows (CF_DIB).
pub enum Imagen {
    Png(Vec<u8>),
    Dib(Vec<u8>),
}

pub enum Fallo {
    /// Más de 50 MB sin comprimir.
    Grande,
    NoSeEscribe,
}

const TOPE: u64 = 50 << 20;
/// Lo que puede ocupar una imagen de un .md mientras se lee, antes de reducirla (en otro hilo y solo un momento).
const TOPE_LEER: u64 = 256 << 20;

fn entero(b: &[u8], en: usize, be: bool) -> Option<u32> {
    let x: [u8; 4] = b.get(en..en + 4)?.try_into().ok()?;
    Some(if be { u32::from_be_bytes(x) } else { u32::from_le_bytes(x) })
}

/// Ancho por alto por 4 bytes: lo que ocupa sin comprimir.
fn sin_comprimir(im: &Imagen) -> Option<u64> {
    let (w, h) = match im {
        Imagen::Png(b) => (entero(b, 16, true)?, entero(b, 20, true)?),
        Imagen::Dib(b) => (entero(b, 4, false)?, (entero(b, 8, false)? as i32).unsigned_abs()),
    };
    Some((w as u64).saturating_mul(h as u64).saturating_mul(4))
}

/// Guarda la imagen junto al .md con un nombre libre y dice cuál. Por un temporal y renombrar sin sustituir: si en ese
/// momento aparece uno con el mismo nombre, se prueba con « 2», « 3»…
pub fn guardar_junto(md: &Path, im: &Imagen) -> Result<String, Fallo> {
    if sin_comprimir(im).is_none_or(|n| n > TOPE) {
        return Err(Fallo::Grande);
    }
    let carpeta = md.parent().ok_or(Fallo::NoSeEscribe)?;
    let base = md.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    let base = format!("{base} {:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    let tmp = carpeta.join(format!("{base}.png.bloc-tmp"));
    // Nuevo y propio: si ya hay uno con ese nombre (otra ventana pegando en el mismo segundo, un enlace puesto ahí), no.
    let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp) else { return Err(Fallo::NoSeEscribe) };
    let escrito = match im {
        Imagen::Png(b) => {
            let bien = std::io::Write::write_all(&mut f, b).is_ok();
            drop(f);
            bien
        }
        Imagen::Dib(b) => {
            drop(f);
            png_de_dib(b, &tmp)
        }
    };
    if !escrito {
        let _ = std::fs::remove_file(&tmp);
        return Err(Fallo::NoSeEscribe);
    }
    for k in 1..1000 {
        let nombre = if k == 1 { format!("{base}.png") } else { format!("{base} {k}.png") };
        let (de, a) = (ws_os(tmp.as_os_str()), ws_os(carpeta.join(&nombre).as_os_str()));
        if unsafe { MoveFileExW(de.as_ptr(), a.as_ptr(), 0) } != 0 {
            return Ok(nombre);
        }
        // Otro error que «ya existe» (ERROR_FILE_EXISTS, ERROR_ALREADY_EXISTS) no se arregla con otro nombre.
        if !matches!(unsafe { GetLastError() }, 80 | 183) {
            break;
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(Fallo::NoSeEscribe)
}

#[repr(C)]
struct Arranque {
    version: u32,
    aviso: usize,
    sin_hilo: i32,
    sin_codecs: i32,
}

/// GDI+ de Windows, cargado y arrancado mientras dura: solo al pegar una imagen o al dibujar una en un .md.
struct Gdip {
    dll: windows_sys::Win32::Foundation::HMODULE,
    ficha: usize,
    f: [usize; NOMBRES.len()],
}

const NOMBRES: [&[u8]; 9] = [
    b"GdiplusStartup\0",
    b"GdiplusShutdown\0",
    b"GdipCreateBitmapFromGdiDib\0",
    b"GdipSaveImageToFile\0",
    b"GdipDisposeImage\0",
    b"GdipCreateBitmapFromFile\0",
    b"GdipGetImageWidth\0",
    b"GdipGetImageHeight\0",
    b"GdipCreateHBITMAPFromBitmap\0",
];

impl Gdip {
    fn abrir() -> Option<Gdip> {
        type Arrancar = unsafe extern "system" fn(*mut usize, *const Arranque, *mut usize) -> i32;
        unsafe {
            let dll = LoadLibraryExW(ws("gdiplus.dll").as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if dll.is_null() {
                return None;
            }
            let mut f = [0usize; NOMBRES.len()];
            for (x, n) in f.iter_mut().zip(NOMBRES) {
                match GetProcAddress(dll, n.as_ptr()) {
                    Some(p) => *x = p as usize,
                    None => {
                        FreeLibrary(dll);
                        return None;
                    }
                }
            }
            let mut ficha = 0usize;
            let arrancar = std::mem::transmute::<usize, Arrancar>(f[0]);
            if arrancar(&mut ficha, &Arranque { version: 1, aviso: 0, sin_hilo: 0, sin_codecs: 1 }, std::ptr::null_mut()) != 0 {
                FreeLibrary(dll);
                return None;
            }
            Some(Gdip { dll, ficha, f })
        }
    }

    /// La función n de NOMBRES con su tipo.
    unsafe fn de<T: Copy>(&self, n: usize) -> T {
        unsafe { std::mem::transmute_copy::<usize, T>(&self.f[n]) }
    }
}

impl Drop for Gdip {
    fn drop(&mut self) {
        type Parar = unsafe extern "system" fn(usize);
        unsafe {
            self.de::<Parar>(1)(self.ficha);
            FreeLibrary(self.dll);
        }
    }
}

type Soltar = unsafe extern "system" fn(usize) -> i32;

/// El mapa de bits (CF_DIB: cabecera, colores y píxeles) a PNG con GDI+.
fn png_de_dib(dib: &[u8], ruta: &Path) -> bool {
    type Crear = unsafe extern "system" fn(*const u8, *const u8, *mut usize) -> i32;
    type Guardar = unsafe extern "system" fn(usize, *const u16, *const windows_sys::core::GUID, *const u8) -> i32;
    // El codificador PNG de GDI+.
    const PNG: windows_sys::core::GUID = windows_sys::core::GUID::from_u128(0x557CF406_1A04_11D3_9A73_0000F81EF32E);
    let (Some(cab), Some(bits), Some(comp)) = (entero(dib, 0, false), dib.get(14..16), entero(dib, 16, false)) else { return false };
    let bpp = u16::from_le_bytes([bits[0], bits[1]]);
    let usados = entero(dib, 32, false).unwrap_or(0) as usize;
    let colores = if usados > 0 { usados } else if bpp <= 8 { 1 << bpp } else { 0 };
    let mascaras = if cab == 40 && comp == 3 { 12 } else { 0 };
    let desde = cab as usize + colores * 4 + mascaras;
    // GDI+ lee la cabecera y los píxeles que dice la cabecera: tienen que estar dentro de lo que trae el portapapeles.
    let (Some(w), Some(h)) = (entero(dib, 4, false), entero(dib, 8, false)) else { return false };
    let fila = (w as u64 * bpp as u64).div_ceil(32) * 4;
    let cabe = matches!(cab, 40 | 108 | 124) && matches!(comp, 0 | 3) && matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) && colores <= 256;
    if !cabe || (w as i32) <= 0 || desde as u64 + fila * (h as i32).unsigned_abs() as u64 > dib.len() as u64 {
        return false;
    }
    let Some(g) = Gdip::abrir() else { return false };
    let mut mapa = 0usize;
    unsafe {
        if g.de::<Crear>(2)(dib.as_ptr(), dib[desde..].as_ptr(), &mut mapa) != 0 {
            return false;
        }
        let bien = g.de::<Guardar>(3)(mapa, ws_os(ruta.as_os_str()).as_ptr(), &PNG, std::ptr::null()) == 0;
        g.de::<Soltar>(4)(mapa);
        bien
    }
}

/// M6: una imagen de un archivo, en un mapa de bits de Windows (HBITMAP) con su ancho y alto, sobre el color `fondo`
/// (COLORREF) donde es transparente, y reducida si no cabe en `caja` (ancho, alto). Nada si GDI+ no la entiende o pasa
/// de TOPE_LEER sin comprimir. Para otro hilo: el mapa de bits va como número.
pub fn cargar(ruta: &Path, fondo: u32, caja: (i32, i32)) -> Option<(usize, i32, i32)> {
    type Crear = unsafe extern "system" fn(*const u16, *mut usize) -> i32;
    type Medida = unsafe extern "system" fn(usize, *mut u32) -> i32;
    type Mapa = unsafe extern "system" fn(usize, *mut usize, u32) -> i32;
    let g = Gdip::abrir()?;
    let mut imagen = 0usize;
    unsafe {
        if g.de::<Crear>(5)(ws_os(ruta.as_os_str()).as_ptr(), &mut imagen) != 0 {
            return None;
        }
        let (mut w, mut h) = (0u32, 0u32);
        g.de::<Medida>(6)(imagen, &mut w);
        g.de::<Medida>(7)(imagen, &mut h);
        // ARGB a partir del COLORREF (0x00BBGGRR).
        let argb = 0xFF00_0000 | (fondo & 0xFF) << 16 | (fondo & 0xFF00) | (fondo >> 16 & 0xFF);
        let mut mapa = 0usize;
        let bien = w > 0 && h > 0 && w as u64 * h as u64 * 4 <= TOPE_LEER && g.de::<Mapa>(8)(imagen, &mut mapa, argb) == 0;
        g.de::<Soltar>(4)(imagen);
        if !bien {
            return None;
        }
        let (w, h) = (w as i32, h as i32);
        let (mw, mh) = (caja.0.max(1), caja.1.max(1));
        if w <= mw && h <= mh {
            return Some((mapa, w, h));
        }
        let (nw, nh) = if w as i64 * mh as i64 >= h as i64 * mw as i64 { (mw, (h as i64 * mw as i64 / w as i64).max(1) as i32) } else { ((w as i64 * mh as i64 / h as i64).max(1) as i32, mh) };
        let pantalla = GetDC(std::ptr::null_mut());
        let (origen, destino) = (CreateCompatibleDC(pantalla), CreateCompatibleDC(pantalla));
        let nuevo = CreateCompatibleBitmap(pantalla, nw, nh);
        ReleaseDC(std::ptr::null_mut(), pantalla);
        let (a0, a1) = (SelectObject(origen, mapa as _), SelectObject(destino, nuevo as _));
        SetStretchBltMode(destino, HALFTONE);
        SetBrushOrgEx(destino, 0, 0, std::ptr::null_mut());
        let bien = !nuevo.is_null() && StretchBlt(destino, 0, 0, nw, nh, origen, 0, 0, w, h, SRCCOPY) != 0;
        SelectObject(origen, a0);
        SelectObject(destino, a1);
        DeleteDC(origen);
        DeleteDC(destino);
        DeleteObject(mapa as _);
        if !bien {
            if !nuevo.is_null() {
                DeleteObject(nuevo as _);
            }
            return None;
        }
        Some((nuevo as usize, nw, nh))
    }
}
