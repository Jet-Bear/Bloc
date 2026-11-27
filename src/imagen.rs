// M5 (1.4): una imagen pegada en un .md se guarda como PNG junto a él («<nombre> AAAA-MM-DD HH-MM-SS.png») y se enlaza.
// Sin subcarpetas. Lo que no viene ya en PNG lo convierte GDI+ de Windows, que se carga solo al pegar una imagen.
use std::path::Path;

use windows_sys::Win32::Foundation::FreeLibrary;
use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::w::ws;

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
    Some(w as u64 * h as u64 * 4)
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
    let escrito = match im {
        Imagen::Png(b) => std::fs::write(&tmp, b).is_ok(),
        Imagen::Dib(b) => png_de_dib(b, &tmp),
    };
    if !escrito {
        let _ = std::fs::remove_file(&tmp);
        return Err(Fallo::NoSeEscribe);
    }
    for k in 1..1000 {
        let nombre = if k == 1 { format!("{base}.png") } else { format!("{base} {k}.png") };
        let (de, a) = (ws(&tmp.to_string_lossy()), ws(&carpeta.join(&nombre).to_string_lossy()));
        if unsafe { MoveFileExW(de.as_ptr(), a.as_ptr(), 0) } != 0 {
            return Ok(nombre);
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

/// El mapa de bits (CF_DIB: cabecera, colores y píxeles) a PNG con GDI+.
fn png_de_dib(dib: &[u8], ruta: &Path) -> bool {
    type Arrancar = unsafe extern "system" fn(*mut usize, *const Arranque, *mut usize) -> i32;
    type Parar = unsafe extern "system" fn(usize);
    type Crear = unsafe extern "system" fn(*const u8, *const u8, *mut usize) -> i32;
    type Guardar = unsafe extern "system" fn(usize, *const u16, *const windows_sys::core::GUID, *const u8) -> i32;
    type Soltar = unsafe extern "system" fn(usize) -> i32;
    // El codificador PNG de GDI+.
    const PNG: windows_sys::core::GUID = windows_sys::core::GUID::from_u128(0x557CF406_1A04_11D3_9A73_0000F81EF32E);
    let (Some(cab), Some(bits), Some(comp)) = (entero(dib, 0, false), dib.get(14..16), entero(dib, 16, false)) else { return false };
    let bpp = u16::from_le_bytes([bits[0], bits[1]]);
    let usados = entero(dib, 32, false).unwrap_or(0) as usize;
    let colores = if usados > 0 { usados } else if bpp <= 8 { 1 << bpp } else { 0 };
    let mascaras = if cab == 40 && comp == 3 { 12 } else { 0 };
    let desde = cab as usize + colores * 4 + mascaras;
    if desde >= dib.len() {
        return false;
    }
    unsafe {
        let gdip = LoadLibraryExW(ws("gdiplus.dll").as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if gdip.is_null() {
            return false;
        }
        let f = |n: &[u8]| GetProcAddress(gdip, n.as_ptr()).map(|p| p as usize);
        let (Some(arrancar), Some(parar), Some(crear), Some(guardar), Some(soltar)) =
            (f(b"GdiplusStartup\0"), f(b"GdiplusShutdown\0"), f(b"GdipCreateBitmapFromGdiDib\0"), f(b"GdipSaveImageToFile\0"), f(b"GdipDisposeImage\0"))
        else {
            FreeLibrary(gdip);
            return false;
        };
        let arrancar = std::mem::transmute::<usize, Arrancar>(arrancar);
        let parar = std::mem::transmute::<usize, Parar>(parar);
        let crear = std::mem::transmute::<usize, Crear>(crear);
        let guardar = std::mem::transmute::<usize, Guardar>(guardar);
        let soltar = std::mem::transmute::<usize, Soltar>(soltar);
        let mut ficha = 0usize;
        let mut bien = false;
        if arrancar(&mut ficha, &Arranque { version: 1, aviso: 0, sin_hilo: 0, sin_codecs: 0 }, std::ptr::null_mut()) == 0 {
            let mut mapa = 0usize;
            if crear(dib.as_ptr(), dib[desde..].as_ptr(), &mut mapa) == 0 {
                bien = guardar(mapa, ws(&ruta.to_string_lossy()).as_ptr(), &PNG, std::ptr::null()) == 0;
                soltar(mapa);
            }
            parar(ficha);
        }
        FreeLibrary(gdip);
        bien
    }
}
