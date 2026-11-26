// Junta el código de Bloc y sus pruebas en un solo bloque comprimido que va dentro de Bloc.exe (ver src/fuentes.rs):
// cada archivo, su ruta (largo en 16 bits y UTF-8) y su contenido (largo en 32 bits), todo comprimido con LZMS, el
// compresor de Windows. Se cogen solos todos los .rs y .py, para que ninguno nuevo se quede fuera.
use std::ffi::c_void;
use std::path::Path;

// cabinet.dll se carga a mano: enlazarla pediría dlltool, que no está en el compilador para el destino gnu.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(nombre: *const u16) -> *mut c_void;
    fn GetProcAddress(dll: *mut c_void, nombre: *const u8) -> *mut c_void;
}

type Crear = unsafe extern "system" fn(u32, *const c_void, *mut isize) -> i32;
type Comprimir = unsafe extern "system" fn(isize, *const u8, usize, *mut u8, usize, *mut usize) -> i32;
type Cerrar = unsafe extern "system" fn(isize) -> i32;

const LZMS: u32 = 5;
const SUELTOS: &[&str] = &["Cargo.toml", "Cargo.lock", "build.rs", "construir.ps1", "PLAN.md", ".gitignore"];
const CARPETAS: &[(&str, &[&str])] = &[("src", &["rs"]), ("herramientas", &["rs"]), ("recursos", &["ico", "manifest", "msix", "cer"]), ("pruebas", &["py"])];

fn comprimir(datos: &[u8]) -> Vec<u8> {
    unsafe {
        let dll = LoadLibraryW("cabinet.dll\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
        assert!(!dll.is_null(), "cabinet.dll");
        let funcion = |n: &str| {
            let f = GetProcAddress(dll, format!("{n}\0").as_ptr());
            assert!(!f.is_null(), "{n}");
            f
        };
        let crear: Crear = std::mem::transmute(funcion("CreateCompressor"));
        let comprimir: Comprimir = std::mem::transmute(funcion("Compress"));
        let cerrar: Cerrar = std::mem::transmute(funcion("CloseCompressor"));
        let mut c = 0isize;
        assert!(crear(LZMS, std::ptr::null(), &mut c) != 0, "CreateCompressor");
        let mut n = 0usize;
        comprimir(c, datos.as_ptr(), datos.len(), std::ptr::null_mut(), 0, &mut n);
        let mut r = vec![0u8; n];
        assert!(comprimir(c, datos.as_ptr(), datos.len(), r.as_mut_ptr(), r.len(), &mut n) != 0, "Compress");
        r.truncate(n);
        cerrar(c);
        r
    }
}

fn main() {
    let raiz = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let raiz = Path::new(&raiz);
    // Una copia sin alguno de estos (Cargo.lock, PLAN.md...) compila igual: el código que lleva dentro va sin él.
    let mut rutas: Vec<String> = SUELTOS.iter().filter(|r| raiz.join(r).exists()).map(|r| r.to_string()).collect();
    for (carpeta, tipos) in CARPETAS {
        println!("cargo:rerun-if-changed={carpeta}");
        let Ok(leida) = std::fs::read_dir(raiz.join(carpeta)) else { continue };
        let mut dentro: Vec<String> = leida
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| tipos.iter().any(|t| n.ends_with(&format!(".{t}"))))
            .map(|n| format!("{carpeta}/{n}"))
            .collect();
        dentro.sort();
        rutas.extend(dentro);
    }
    let mut todo = Vec::new();
    for r in &rutas {
        println!("cargo:rerun-if-changed={r}");
        let datos = std::fs::read(raiz.join(r)).expect(r);
        todo.extend((r.len() as u16).to_le_bytes());
        todo.extend(r.as_bytes());
        todo.extend((datos.len() as u32).to_le_bytes());
        todo.extend(datos);
    }
    let destino = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("fuentes.lzms");
    std::fs::write(destino, comprimir(&todo)).expect("fuentes.lzms");
}
