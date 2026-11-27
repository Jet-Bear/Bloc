#![windows_subsystem = "windows"]

mod barras;
mod buscar;
mod com;
mod config;
mod copia;
mod dialogo;
mod editor;
mod eleccion;
mod fichero;
mod fuentes;
mod historial;
mod html;
mod idiomas;
mod imagen;
mod imprimir;
mod informe;
mod instalar;
mod md;
mod menu;
mod nota;
mod previa;
mod re;
mod tabla;
mod tema;
mod textos;
mod ventana;
mod w;

fn main() {
    std::panic::set_hook(Box::new(|i| w::apuntar(&i.to_string())));
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "/fuentes") {
        std::process::exit(fuentes::sacar(args.into_iter().nth(1)));
    }
    // «Editar con Bloc» del menú corto: el paquete de Bloc lo abre así, y COM puede añadirle -Embedding.
    if args.iter().any(|a| a == "-Menu") {
        return menu::servir();
    }
    match args.first().and_then(|a| a.to_str()) {
        // La vista previa del Explorador: COM lo abre así, y no es para enseñar ninguna ventana propia.
        Some("-Embedding") => return previa::servir(),
        Some("/desinstalar") => return instalar::desinstalar(),
        // Como administrador, con el aviso de Windows: para el «Editar con Bloc» del menú corto.
        Some("/certificado") => std::process::exit(instalar::fiar_certificado(true)),
        Some("/sin-certificado") => std::process::exit(instalar::fiar_certificado(false)),
        Some("/lista") => return instalar::lista_de_tareas(),
        // Windows lo vuelve a abrir así tras reiniciar por una actualización: solo si hay una nota que abrir, y si no la
        // abre ya el inicio con Windows.
        Some("/reinicio") if !nota::libre() || instalar::inicio_con_windows() => return,
        Some("/reinicio") => args.clear(),
        _ => {}
    }
    // Las pruebas ponen BLOC_SIN_INSTALAR para probar el .exe recién compilado sin instalarlo.
    if std::env::var_os("BLOC_SIN_INSTALAR").is_none() && instalar::instalar(&args) {
        return;
    }
    let instalado = args.first().is_some_and(|a| a == "/instalado");
    if instalado {
        args.remove(0);
    }
    std::process::exit(ventana::ejecutar(args.into_iter().next(), instalado));
}
