// Preferencias en %APPDATA%\Bloc\bloc.ini. Si no se pueden leer o guardar, se sigue con las de siempre.
use std::path::{Path, PathBuf};

/// Archivo › Recientes, como mucho.
pub const MAX_RECIENTES: usize = 8;

pub struct Config {
    pub ajuste: bool,
    pub barra: bool,
    pub max: bool,
    pub noche: bool,
    pub corrector: bool,
    /// Las marcas de Markdown a la vista; si no, se ve solo el resultado.
    pub marcas: bool,
    /// Los documentos sin archivo empiezan como Markdown, con formato.
    pub nuevos_md: bool,
    /// Archivo › Guardar copia mientras escribes (ver copia.rs).
    pub copia: bool,
    /// 0 estrechos, 1 normales, 2 anchos, 3 muy anchos.
    pub margenes: u16,
    pub zoom: i32,
    /// Posición y tamaño de la ventana sin maximizar: x, y, ancho, alto.
    pub marco: Option<(i32, i32, i32, i32)>,
    pub letra: String,
    pub tamano: i32,
    /// Archivo › Recientes, el último arriba. No es de cada ventana: se escribe enseguida (ver `cambiar_recientes`).
    recientes: Vec<String>,
    /// Lo que había en el archivo: si no cambia nada, al cerrar no se vuelve a escribir.
    leido: String,
}

// Las pruebas ponen BLOC_INI para no tocar la configuración de quien usa Bloc a la vez.
pub fn ruta() -> Option<PathBuf> {
    std::env::var_os("BLOC_INI")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Bloc").join("bloc.ini")))
}

/// La carpeta de Bloc del perfil (la del bloc.ini): ahí va también la nota para la próxima vez.
pub fn carpeta() -> Option<PathBuf> {
    ruta()?.parent().map(Path::to_path_buf)
}

impl Config {
    pub fn cargar() -> Config {
        Config::leer(ruta().and_then(|r| std::fs::read_to_string(r).ok()).unwrap_or_default())
    }

    fn leer(leido: String) -> Config {
        let mut c = Config {
            ajuste: true,
            barra: true,
            max: false,
            noche: false,
            corrector: true,
            marcas: false,
            nuevos_md: false,
            copia: true,
            margenes: 1,
            zoom: 100,
            marco: None,
            letra: "Consolas".into(),
            tamano: 11,
            recientes: Vec::new(),
            leido,
        };
        let mut noche = None;
        for l in c.leido.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            let n: i32 = v.trim().parse().unwrap_or(0);
            match k {
                "ajuste" => c.ajuste = n != 0,
                "barra" => c.barra = n != 0,
                "max" => c.max = n != 0,
                "noche" => noche = Some(n != 0),
                "corrector" => c.corrector = n != 0,
                "marcas" => c.marcas = n != 0,
                "nuevos_md" => c.nuevos_md = n != 0,
                "copia" => c.copia = n != 0,
                "margenes" if (0..4).contains(&n) => c.margenes = n as u16,
                "zoom" if (30..=500).contains(&n) => c.zoom = n,
                "tamano" if (6..=72).contains(&n) => c.tamano = n,
                "letra" if !v.trim().is_empty() && v.len() < 32 => c.letra = v.trim().into(),
                "reciente" if !v.trim().is_empty() && c.recientes.len() < MAX_RECIENTES => c.recientes.push(v.trim().into()),
                "marco" => {
                    let p: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if p.len() == 4 {
                        c.marco = Some((p[0], p[1], p[2], p[3]));
                    }
                }
                _ => {}
            }
        }
        // La primera vez, como tenga Windows las aplicaciones.
        c.noche = noche.unwrap_or_else(crate::tema::como_windows);
        c
    }

    /// Escribe lo que ha cambiado esta ventana encima de lo que haya ahora en el archivo: así no se pierde lo que haya
    /// cambiado otra ventana cerrada antes. Primero en otro archivo, para que un corte no deje el ini vacío.
    pub fn guardar(&self) {
        let Some(r) = ruta() else { return };
        let antes = Config::leer(self.leido.clone());
        let mut c = Config::leer(std::fs::read_to_string(&r).unwrap_or_default());
        macro_rules! mios {
            ($($x:ident),*) => { $( if self.$x != antes.$x { c.$x = self.$x; } )* };
        }
        mios!(ajuste, barra, max, noche, corrector, marcas, nuevos_md, copia, margenes, zoom, marco, tamano);
        if self.letra != antes.letra {
            c.letra = self.letra.clone();
        }
        c.escribir(&r);
    }

    fn escribir(&self, r: &Path) {
        let texto = self.texto();
        if texto == self.leido {
            return;
        }
        if let Some(d) = r.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        // Uno por proceso: dos ventanas que se cierran a la vez no escriben en el mismo.
        let nuevo = r.with_extension(format!("ini.{}.nuevo", std::process::id()));
        if std::fs::write(&nuevo, texto).is_ok() && std::fs::rename(&nuevo, r).is_err() {
            let _ = std::fs::remove_file(&nuevo);
        }
    }

    fn texto(&self) -> String {
        let (x, y, w, h) = self.marco.unwrap_or_default();
        let b = |v: bool| v as u8;
        format!(
            "ajuste={}\r\nbarra={}\r\nmax={}\r\nzoom={}\r\nmarco={x},{y},{w},{h}\r\nnoche={}\r\ncorrector={}\r\nmarcas={}\r\nnuevos_md={}\r\ncopia={}\r\nmargenes={}\r\nletra={}\r\ntamano={}\r\n",
            b(self.ajuste), b(self.barra), b(self.max), self.zoom, b(self.noche), b(self.corrector), b(self.marcas), b(self.nuevos_md), b(self.copia), self.margenes, self.letra, self.tamano
        ) + &self.recientes.iter().map(|r| format!("reciente={r}\r\n")).collect::<String>()
    }
}

/// La lista de ahora, con lo que haya puesto cualquier ventana. Sin mirar si siguen ahí: una carpeta de red apagada
/// congelaría el menú.
pub fn recientes() -> Vec<PathBuf> {
    Config::cargar().recientes.iter().map(PathBuf::from).collect()
}

/// Cambia la lista en el archivo en el momento, sobre la que haya: así la ven las demás ventanas, y lo que guarde
/// cualquiera al cerrar no la toca.
fn cambiar_recientes(f: impl FnOnce(&mut Vec<String>)) {
    let Some(r) = ruta() else { return };
    let mut c = Config::leer(std::fs::read_to_string(&r).unwrap_or_default());
    f(&mut c.recientes);
    c.recientes.truncate(MAX_RECIENTES);
    c.escribir(&r);
}

fn es(a: &str, b: &Path) -> bool {
    b.to_str().is_some_and(|b| a.to_lowercase() == b.to_lowercase())
}

/// Abierto o guardado con otro nombre: arriba del todo.
pub fn poner_reciente(p: &Path) {
    let Some(s) = p.to_str() else { return };
    cambiar_recientes(|l| {
        l.retain(|x| !es(x, p));
        l.insert(0, s.to_string());
    });
}

pub fn quitar_reciente(p: &Path) {
    cambiar_recientes(|l| l.retain(|x| !es(x, p)));
}

pub fn borrar_recientes() {
    cambiar_recientes(Vec::clear);
}

#[cfg(test)]
mod pruebas {
    use super::Config;

    #[test]
    fn recientes_en_el_ini() {
        let mut texto = "noche=0\r\nreciente=A:\\x=1.md\r\nreciente=C:\\b.txt\r\n".to_string();
        for i in 0..10 {
            texto += &format!("reciente=C:\\{i}.txt\r\n");
        }
        let c = Config::leer(texto);
        assert_eq!(c.recientes.len(), super::MAX_RECIENTES, "como mucho 8");
        assert_eq!(c.recientes[0], r"A:\x=1.md", "un = en la ruta no la corta");
        let otra = Config::leer(c.texto());
        assert_eq!(otra.recientes, c.recientes, "se escriben como se leen, en orden");
    }
}
