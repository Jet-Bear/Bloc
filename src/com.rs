// COM a mano: solo las funciones que hacen falta de cada interfaz, por su número en la tabla. Sin el crate de Windows,
// que pesa mucho más de lo que Bloc usa.
use std::ffi::c_void;

use windows_sys::core::GUID;
use windows_sys::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

/// Un puntero a una interfaz, que se suelta al acabar.
pub struct Com(pub *mut c_void);

// Las interfaces que pasan de un hilo a otro (las de imprimir) son de las que lo admiten: Direct2D multihilo y los
// objetos de Windows que llaman desde sus hilos.
unsafe impl Send for Com {}

impl Com {
    pub fn crear(clsid: &GUID, iid: &GUID) -> Option<Com> {
        let mut p = std::ptr::null_mut();
        let r = unsafe { CoCreateInstance(clsid, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, iid, &mut p) };
        (r >= 0 && !p.is_null()).then(|| Com(p))
    }

    /// Lo que devuelve una función de la forma `f(…, **salida)`, si va bien.
    pub fn de(r: i32, p: *mut c_void) -> Option<Com> {
        (r >= 0 && !p.is_null()).then(|| Com(p))
    }

    pub fn metodo(&self, n: usize) -> usize {
        unsafe { *(*(self.0 as *const *const usize)).add(n) }
    }

    pub fn pedir(&self, iid: &GUID) -> Option<Com> {
        let qi: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(self.metodo(0)) };
        let mut p = std::ptr::null_mut();
        Com::de(unsafe { qi(self.0, iid, &mut p) }, p)
    }

    pub fn texto(&self, n: usize, s: &[u16]) -> i32 {
        let f: unsafe extern "system" fn(*mut c_void, *const u16) -> i32 = unsafe { std::mem::transmute(self.metodo(n)) };
        unsafe { f(self.0, s.as_ptr()) }
    }

    /// `f(this, **salida)`: lo que da una función sin más datos que la salida.
    pub fn dar(&self, n: usize) -> Option<Com> {
        let f: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 = unsafe { std::mem::transmute(self.metodo(n)) };
        let mut p = std::ptr::null_mut();
        Com::de(unsafe { f(self.0, &mut p) }, p)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        let release: unsafe extern "system" fn(*mut c_void) -> u32 = unsafe { std::mem::transmute(self.metodo(2)) };
        unsafe { release(self.0) };
    }
}
