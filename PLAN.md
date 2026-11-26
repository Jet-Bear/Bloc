# Bloc nativo (Rust) — plan y decisiones

Copia de trabajo del Bloc nativo. La carpeta de origen
solo lleva `Bloc.exe`, que se instala solo y lleva dentro su código (`Bloc.exe /fuentes <carpeta>` lo saca).
`referencia\` es el Bloc de C# (27-28/10/2026) del que sale este: no va al repositorio.

**Alcance:** todo lo del Bloc de C# menos la vista previa de dos paneles (Internet Explorer), más la vista única de
los .md con botones de formato (solo en los .md), imprimir, elegir la letra, elegir codificación y saltos, corrector
ortográfico de Windows, `.LOG`, interfaz en el idioma de Windows (es, en, fr, de, it, pt, ru, pl, tr, zh, ja, ko;
si no está, inglés), una ventana clara y otra oscura a la vez.
**Hecho cuando:** `Bloc.exe` solo en su carpeta, instalado, pasa las pruebas, abre más rápido y con menos memoria que
el de C# (0,16-0,2 s y 35 MB). GitHub va después, con el visto bueno del autor.

## Decisiones (decididas por mí, se pueden cambiar)

- Rust estable, `x86_64-pc-windows-gnu` (sin Visual Studio), `windows-sys` 0.59 como única dependencia: la 0.61 enlaza
  con raw-dylib y pide `dlltool`, que esta instalación mínima no trae.
- Un solo control de texto para todo: RichEdit 5 (`msftedit.dll`, clase `RICHEDIT50W`).
  - Texto plano (.txt y el resto): sin formato, deshacer nativo de RichEdit (varios niveles, no cuesta nada), sin
    copia del texto en memoria.
  - Markdown (hasta `LIMITE_VIVO` caracteres): formato en vivo, deshacer propio (`historial.rs`, como el de C#) y copia
    del texto para saber qué ha cambiado. Por encima del límite, como texto plano con la barra de formato.
- Posiciones: RichEdit cuenta cada salto como 1 carácter (CR). Dentro, todo va con CR; al guardar se ponen los
  saltos del archivo (CRLF, LF o CR).
- Al abrir se comprueba que lo leído del control es exactamente lo cargado; si no (nulos y algún carácter raro), se
  avisa como hoy con los nulos y se pide confirmar al guardar.
- Barras de formato y de estado pintadas por Bloc (iguales en claro y oscuro). Menús de Windows (en oscuro, la barra
  de menús la pinta Bloc con WM_UAHDRAWMENU/ITEM como en C#).
- Codificación y saltos: submenús de Archivo y clic en su parte de la barra de estado.
- Letra: solo para texto plano (como el Bloc de notas); los .md tienen su aspecto propio (Segoe UI) y siguen el zoom.
- Recursos (icono, manifiesto) se ponen tras compilar con `UpdateResource` (`herramientas\sellar.rs`, lo llama `construir.ps1`): sin `windres`.

## Lo averiguado de RichEdit (prototipos en C#, 28/10)

- `EM_GETTEXTEX` (GT_DEFAULT, 1200) da el texto con CR que cuadra con las posiciones; WM_GETTEXT da CRLF y no cuadra.
- Insertar `"\r\n"` o `"\n"` hace un solo párrafo.
- Pintar una línea: seleccionar `[inicio, fin)` sin su salto y meter RTF por `EM_STREAMIN` con `SFF_SELECTION`:
  ~1,7 ms con el cursor y el desplazamiento guardados. `EM_SETCHARFORMAT` por tramos es más lento (0,2-0,8 ms cada uno).
- Un `\par` final se lo come RichEdit al meter RTF al final del documento: no pintar nunca una última línea vacía.
- `CFE_HIDDEN` mueve la selección: las marcas ocultas van en `\fs2` y del color del fondo. Ocultas en todas las líneas,
  también en la del cursor (29/10): enseñarlas al llegar el cursor movía la línea entera al hacer clic. «Ver › Mostrar
  marcas de formato» (Ctrl+Mayús+M) las enseña tenues y a su tamaño; al imprimir, siempre ocultas.
- Botones de formato con iconos: los de la letra de iconos de Windows (Segoe Fluent Icons; en Windows 10, Segoe MDL2
  Assets, mismos códigos) y dibujados por Bloc los que no trae (H1-H3, lista numerada, cita). El nombre, en la ayuda.
- `EM_SETUNDOLIMIT 0` en los .md (deshacer propio); `DetectUrls` apagado; `ES_NOOLEDRAGDROP`.
- Pegar siempre como texto (interceptar WM_PASTE, Mayús+Insert); copiar/cortar el texto tal cual (WM_COPY/WM_CUT,
  Ctrl+Insert, Mayús+Supr). Tragar los atajos de Ctrl de RichEdit que cambian formato (Ctrl+E/J/L/R, Ctrl+Mayús+...).
- Ctrl+rueda: zoom propio (`EM_SETZOOM`), no el de RichEdit.

## Lo averiguado al probar el nativo (28/10)

- `richedit.h` empaqueta sus estructuras a 4 bytes: `GETTEXTEX` y `EDITSTREAM` llevan `repr(C, packed(4))`.
- `DrawTextW` lee el puntero aunque el largo sea 0: un `Vec` vacío no se le pasa.
- `EM_SETCHARFORMAT` con letra y `CFM_CHARSET` a la vez revisa carácter a carácter (38 s con 5 MB): sin `CFM_CHARSET`.
  Y al cargar, el control se vacía antes de dar formato.
- RichEdit solo salta de línea con la tecla Intro: el Intro que llega solo como carácter lo mete Bloc.
- Su deshacer junta todo lo escrito seguido: `EM_STOPGROUPTYPING` en cada espacio tras algo que no lo es (por palabras,
  como el `historial.rs` de los .md).
- El corrector tarda ~75 ms en ponerse en marcha: se enciende con la ventana ya pintada. En un escritorio oculto no
  subraya nada (Windows lo enciende con el foco): se comprueba a la vista.
- Un documento sin archivo que vuelve a quedar vacío no cuenta como cambiado: ni asterisco ni pregunta al cerrar.
- En las pruebas, el retroceso va con `WM_KEYDOWN`: el carácter 8 por `WM_CHAR` no borra en RichEdit.
- Ctrl+S en el mismo archivo escribe en otro hilo (30/10): con el disco saturado, `sync_all` y `ReplaceFileW` tardaban
  de 1,4 a 5,7 s con la ventana congelada. Lo escrito mientras queda por guardar. Guardar como, el «¿Guardar
  cambios?» y el apagado esperan al resultado; cerrar, abrir o «Nuevo» esperan antes a que acabe el de fondo.
- Los diálogos de abrir, guardar como y letra son los de Windows: 0,5-1,2 s en un PC cargado, igual cada vez (recorren
  la carpeta o las fuentes instaladas). Aceptar la misma letra no vuelve a dar formato (2 s en 5 MB).
- «Ver › Nuevos documentos en Markdown» (30/10, pedido por el autor): los documentos sin archivo empiezan como Markdown y
  al guardar se propone `.md`; al marcarlo, el sin título que esté abierto cambia con lo que lleve escrito.

## Pruebas

- `cargo test`: archivos (codificaciones, saltos, guardado seguro), deshacer, Markdown (pintado y tablas).
- `pruebas\prueba_ventana.py <exe> <carpeta>` y `pruebas\prueba_instalar.py <exe> <carpeta>`: en escritorio oculto
  y con su propio ini (`BLOC_INI`); `BLOC_SIN_INSTALAR=1` para no instalar.
