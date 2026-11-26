// El código de Bloc va dentro de Bloc.exe, comprimido (no ocupa memoria: Windows solo lee del disco lo que se usa).
// `Bloc.exe /fuentes <carpeta>` lo saca, listo para compilar con construir.ps1.
use std::ffi::{c_void, OsString};
use std::path::PathBuf;

use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MB_OK};

use crate::textos::t;
use crate::w::{con, funcion, mensaje};

// Lo junta y lo comprime build.rs: cada archivo, su ruta y su contenido.
const COMPRIMIDO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fuentes.lzms"));

/// Los archivos del código, descomprimidos con el compresor de Windows (cabinet.dll, que solo se carga aquí).
fn archivos() -> Option<Vec<(String, Vec<u8>)>> {
    type Crear = unsafe extern "system" fn(u32, *const c_void, *mut isize) -> i32;
    type Descomprimir = unsafe extern "system" fn(isize, *const u8, usize, *mut u8, usize, *mut usize) -> i32;
    type Cerrar = unsafe extern "system" fn(isize) -> i32;
    let (crear, descomprimir, cerrar) = unsafe {
        (
            std::mem::transmute::<usize, Crear>(funcion("cabinet.dll", "CreateDecompressor")?),
            std::mem::transmute::<usize, Descomprimir>(funcion("cabinet.dll", "Decompress")?),
            std::mem::transmute::<usize, Cerrar>(funcion("cabinet.dll", "CloseDecompressor")?),
        )
    };
    let todo = unsafe {
        let mut d = 0isize;
        if crear(5, std::ptr::null(), &mut d) == 0 {
            return None;
        }
        let mut n = 0usize;
        descomprimir(d, COMPRIMIDO.as_ptr(), COMPRIMIDO.len(), std::ptr::null_mut(), 0, &mut n);
        let mut r = vec![0u8; n];
        let bien = descomprimir(d, COMPRIMIDO.as_ptr(), COMPRIMIDO.len(), r.as_mut_ptr(), r.len(), &mut n) != 0;
        cerrar(d);
        bien.then_some(r)?
    };
    let mut r = Vec::new();
    let mut resto = &todo[..];
    while !resto.is_empty() {
        let largo = u16::from_le_bytes(resto.get(..2)?.try_into().ok()?) as usize;
        let ruta = String::from_utf8(resto.get(2..2 + largo)?.to_vec()).ok()?;
        resto = &resto[2 + largo..];
        let largo = u32::from_le_bytes(resto.get(..4)?.try_into().ok()?) as usize;
        r.push((ruta, resto.get(4..4 + largo)?.to_vec()));
        resto = &resto[4 + largo..];
    }
    Some(r)
}

pub fn sacar(carpeta: Option<OsString>) -> i32 {
    let Some(archivos) = archivos() else {
        mensaje(std::ptr::null_mut(), t().fuentes_mal, MB_OK | MB_ICONWARNING);
        return 1;
    };
    let base = carpeta.map(PathBuf::from).or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    let base = std::path::absolute(&base).unwrap_or(base);
    // Nunca se pisa nada: si ya hay algo con el mismo nombre, no se escribe ningún archivo.
    let ya: Vec<String> = archivos.iter().filter(|(r, _)| base.join(r).exists()).map(|(r, _)| r.to_string()).collect();
    if !ya.is_empty() {
        mensaje(std::ptr::null_mut(), &con(t().fuentes_existen, ya.join("\n")), MB_OK | MB_ICONWARNING);
        return 1;
    }
    let escribir = || -> std::io::Result<()> {
        for (r, datos) in &archivos {
            let destino = base.join(r);
            if let Some(p) = destino.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&destino, datos)?;
        }
        Ok(())
    };
    match escribir() {
        Ok(()) => {
            mensaje(std::ptr::null_mut(), &con(t().fuentes_ok, base.display()), MB_OK | MB_ICONINFORMATION);
            0
        }
        Err(e) => {
            mensaje(std::ptr::null_mut(), &format!("{}\n\n{e}", base.display()), MB_OK | MB_ICONWARNING);
            1
        }
    }
}
