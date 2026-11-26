// Pone en Bloc.exe ya compilado el icono, el manifiesto (controles modernos, DPI, rutas largas) y los datos de
// versión. Se hace después de compilar porque el compilador de Rust para Windows (GNU) no trae `windres`.
// Uso: sellar <Bloc.exe> <bloc.ico> <bloc.manifest> [versión]   (las pruebas ponen otra versión; si no, la de Cargo.toml)
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::System::LibraryLoader::{BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const RT_MANIFEST: u16 = 24;

fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Las imágenes del .ico (cada una, un RT_ICON) y el grupo que las reúne (RT_GROUP_ICON 1).
fn icono(ico: &[u8]) -> Result<(Vec<Vec<u8>>, Vec<u8>), String> {
    if ico.len() < 6 || u16le(ico, 2) != 1 {
        return Err("el icono no es un .ico".into());
    }
    let n = u16le(ico, 4) as usize;
    let mut imagenes = Vec::new();
    let mut grupo = ico[..6].to_vec();
    for i in 0..n {
        let e = 6 + i * 16;
        let entrada = ico.get(e..e + 16).ok_or("icono cortado")?;
        let (largo, desde) = (u32le(entrada, 8) as usize, u32le(entrada, 12) as usize);
        imagenes.push(ico.get(desde..desde + largo).ok_or("icono cortado")?.to_vec());
        grupo.extend_from_slice(&entrada[..12]);
        grupo.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    Ok((imagenes, grupo))
}

fn ancho(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn rellenar(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// Un bloque de VS_VERSIONINFO: largo, largo del valor, tipo (1 si es texto), clave, valor e hijos, cada parte
/// alineada a 4 bytes.
fn bloque(clave: &str, valor: &[u8], largo_valor: u16, texto: bool, hijos: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0, 0];
    v.extend_from_slice(&largo_valor.to_le_bytes());
    v.extend_from_slice(&(texto as u16).to_le_bytes());
    v.extend(ancho(clave));
    rellenar(&mut v);
    v.extend_from_slice(valor);
    for h in hijos {
        rellenar(&mut v);
        v.extend_from_slice(h);
    }
    let n = v.len() as u16;
    v[..2].copy_from_slice(&n.to_le_bytes());
    v
}

fn version(ver: &str) -> Vec<u8> {
    let mut p: Vec<u32> = ver.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    p.resize(3, 0);
    let (ms, ls) = (p[0] << 16 | p[1], p[2] << 16);
    let fijo: Vec<u8> = [0xFEEF04BD, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0]
        .iter()
        .flat_map(|x: &u32| x.to_le_bytes())
        .collect();
    let cadena = |k: &str, val: &str| bloque(k, &ancho(val), val.encode_utf16().count() as u16 + 1, true, &[]);
    let tabla = bloque(
        "040904B0",
        &[],
        0,
        true,
        &[
            cadena("FileDescription", "Bloc"),
            cadena("FileVersion", ver),
            cadena("InternalName", "Bloc"),
            cadena("LegalCopyright", "MIT"),
            cadena("OriginalFilename", "Bloc.exe"),
            cadena("ProductName", "Bloc"),
            cadena("ProductVersion", ver),
        ],
    );
    let textos = bloque("StringFileInfo", &[], 0, true, &[tabla]);
    let idioma = bloque("Translation", &[0x09, 0x04, 0xB0, 0x04], 4, false, &[]);
    let variables = bloque("VarFileInfo", &[], 0, true, &[idioma]);
    bloque("VS_VERSION_INFO", &fijo, fijo.len() as u16, false, &[textos, variables])
}

fn sellar(exe: &str, ico: &str, manifiesto: &str, ver: &str) -> Result<(), String> {
    let (imagenes, grupo) = icono(&std::fs::read(ico).map_err(|e| format!("{ico}: {e}"))?)?;
    let man = std::fs::read(manifiesto).map_err(|e| format!("{manifiesto}: {e}"))?;
    let ruta: Vec<u16> = std::ffi::OsStr::new(exe).encode_wide().chain([0]).collect();
    unsafe {
        let h = BeginUpdateResourceW(ruta.as_ptr(), 0);
        if h.is_null() {
            return Err(format!("{exe}: {}", std::io::Error::last_os_error()));
        }
        let poner = |tipo: u16, id: u16, datos: &[u8]| {
            UpdateResourceW(h, tipo as usize as *const u16, id as usize as *const u16, 0, datos.as_ptr() as *const _, datos.len() as u32) != 0
        };
        let mut bien = true;
        for (i, img) in imagenes.iter().enumerate() {
            bien &= poner(RT_ICON, i as u16 + 1, img);
        }
        bien &= poner(RT_GROUP_ICON, 1, &grupo);
        bien &= poner(RT_MANIFEST, 1, &man);
        bien &= poner(RT_VERSION, 1, &version(ver));
        if EndUpdateResourceW(h, (!bien) as i32) == 0 || !bien {
            return Err(format!("{exe}: {}", std::io::Error::last_os_error()));
        }
    }
    Ok(())
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&a.len()) {
        eprintln!("Uso: sellar <Bloc.exe> <bloc.ico> <bloc.manifest> [versión]");
        std::process::exit(2);
    }
    if let Err(e) = sellar(&a[0], &a[1], &a[2], a.get(3).map_or(env!("CARGO_PKG_VERSION"), |v| v)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
