# pyright: reportUndefinedVariable=false
"""Cada botón, opción del menú Formato y tecla de edición de Bloc en cada sitio de un .md (texto, viñeta, numerada,
casilla, cita, título, tabla, código, con formato ya puesto, línea vacía) y con cada selección (cursor, palabra, línea,
dos líneas), con las marcas ocultas y a la vista; y en un .txt. En escritorio oculto y con su propio ini.
Uso: python prueba_botones.py <Bloc.exe> <carpeta> [informe.md]

Dos partes:
- CASOS: lo que tiene que pasar, escrito a mano (⟦ ⟧ es la selección y ‸ el cursor). Un caso que no da lo esperado es
  un FALLO.
- La tabla de todas las combinaciones, con lo que hace cada una y tres avisos que no siempre son fallos: se pierde o
  cambia texto (sin contar marcas), pulsado dos veces no deja el texto como estaba, lo seleccionado no sigue
  seleccionado. Va al informe para repasarla a ojo."""
import ctypes, os, re, sys, threading, time

exe, dir = sys.argv[1:3]
informe = sys.argv[3] if len(sys.argv) > 3 else os.path.join(dir, "botones.md")
sys.argv = ["x", exe, dir, "0", "1"]
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "prueba_azar.py"), encoding="utf-8") as f:
    fuente = f.read()
exec(compile(fuente[:fuente.index("hilo = threading.Thread")], "prueba_azar", "exec"))
VK_TAB = 9

BOTONES = {"negrita": NEGRITA, "cursiva": CURSIVA, "tachado": TACHADO, "código": CODIGO, "enlace": ENLACE,  # noqa: F821
           "H1": TITULO1, "H2": TITULO2, "H3": TITULO3, "viñetas": LISTA, "numerada": NUMERADA,  # noqa: F821
           "casillas": CASILLAS, "cita": CITA, "tabla": TABLA, "alinear": ALINEAR}  # noqa: F821
TECLAS = {"intro": VK_RETURN, "retroceso": VK_BACK, "tab": VK_TAB, "supr": VK_DELETE, "izquierda": 0x25, "abajo": 0x28}  # noqa: F821
MENU = {"deshacer": DESHACER, "rehacer": REHACER, "ajuste": AJUSTE}  # noqa: F821
GUARDAR = 1003
CONMUTAN = {"negrita", "cursiva", "tachado", "código", "H1", "H2", "H3", "viñetas", "numerada", "casillas", "cita"}

# Lo que tiene que pasar, fila a fila de la fase 9.2 de PLAN-REVISION.md: (doc, marcas a la vista, antes, qué, después).
# Marcas 2: ocultas y sin ajuste de línea.
# «qué×2»: pulsado dos veces seguidas; «a+b»: una cosa y luego otra. Además, en cada uno con las marcas ocultas, lo
# pintado tiene que verse igual que repintado entero.
T3 = "| Columna 1 | Columna 2 | Columna 3 |\n| --------- | --------- | --------- |\n|           |           |           |"
T3_SEL = T3.replace("Columna 1", "⟦Columna 1⟧", 1)
CASOS = [
    # Negrita, cursiva, tachado, código
    ("md", 0, "Hola ⟦mundo⟧ bonito", "negrita", "Hola **⟦mundo⟧** bonito"),
    ("md", 0, "Hola **mu‸ndo** bonito", "negrita", "Hola mu‸ndo bonito"),
    ("md", 0, "Hola ‸", "cursiva", "Hola *‸*"),
    ("md", 0, "| a | ⟦b⟧ |\n| - | - |", "negrita", "| a | **⟦b⟧** |\n| - | - |"),
    ("md", 0, "```\nHola mu‸ndo\n```", "negrita", "```\nHola mu‸ndo\n```"),
    ("md", 0, "```\nHola ⟦mundo⟧\n```", "código", "```\nHola ⟦mundo⟧\n```"),
    ("md", 0, "| ⟦a | b |\n|:--|--:|\n| 1⟧ | 2 |", "negrita", "| **⟦a** | **b** |\n|:--|--:|\n| **1⟧** | 2 |"),
    ("md", 0, "| ⟦a | b |\n|:--|--:|\n| 1⟧ | 2 |", "negrita×2", "| ⟦a | b |\n|:--|--:|\n| 1⟧ | 2 |"),
    # Enlace
    ("md", 0, "Hola ⟦mundo⟧", "enlace", "Hola [mundo](‸)"),
    ("md", 0, "⟦https://a.es⟧", "enlace", "[‸](https://a.es)"),
    ("md", 0, "```\nHola ⟦mundo⟧\n```", "enlace", "```\nHola ⟦mundo⟧\n```"),
    ("md", 0, "| ⟦a | b⟧ |\n| - | - |", "enlace", "| ⟦a | b⟧ |\n| - | - |"),
    # Títulos
    ("md", 0, "Hola m‸undo", "H2", "## Hola m‸undo"),
    ("md", 0, "## Hola m‸undo", "H2", "Hola m‸undo"),
    ("md", 0, "## Hola m‸undo", "H1", "# Hola m‸undo"),
    ("md", 0, "⟦Hola mundo⟧", "H2", "## ⟦Hola mundo⟧"),
    ("md", 0, "Hola ⟦mundo⟧ bonito", "H2", "Hola\n## ⟦mundo⟧\nbonito"),
    ("md", 0, "⟦Hola⟧ mundo", "H1", "# ⟦Hola⟧\nmundo"),
    ("md", 0, "Hola ⟦mundo⟧", "H3", "Hola\n### ⟦mundo⟧"),
    ("md", 0, "Antes\nHola ⟦mundo⟧ bonito\nDespués", "H2", "Antes\nHola\n## ⟦mundo⟧\nbonito\nDespués"),
    ("md", 1, "Hola ⟦mundo\nbonito⟧ día", "H2", "## Hola ⟦mundo\n## bonito⟧ día"),
    ("md", 0, "Hola ⟦mundo\nbonito⟧ día", "H2", "## Hola ⟦mundo bonito⟧ día"),
    ("md", 0, "## Hola ⟦mundo⟧", "H1", "# Hola ⟦mundo⟧"),
    ("md", 0, "- Hola m‸undo", "H2", "## Hola m‸undo"),
    ("md", 0, "1. Hola m‸undo", "H1", "# Hola m‸undo"),
    ("md", 0, "- [ ] Hola m‸undo", "H3", "### Hola m‸undo"),
    ("md", 0, "- Hola ⟦mundo⟧ bonito", "H2", "- Hola\n## ⟦mundo⟧\nbonito"),
    ("md", 0, "> Hola m‸undo", "H2", "> ## Hola m‸undo"),
    ("md", 0, "> ## Hola m‸undo", "H2", "> Hola m‸undo"),
    ("md", 0, "> Hola ⟦mundo⟧ bonito", "H2", "> Hola\n> ## ⟦mundo⟧\n> bonito"),
    ("md", 0, "| a | b‸ |\n| - | - |", "H2", "| a | b‸ |\n| - | - |"),
    ("md", 0, "```\nHola‸\n```", "H1", "```\nHola‸\n```"),
    # Viñetas, numerada, casillas
    ("md", 0, "Hola m‸undo", "viñetas", "- Hola m‸undo"),
    ("md", 0, "- Hola m‸undo", "viñetas", "Hola m‸undo"),
    ("md", 0, "- Hola m‸undo", "numerada", "1. Hola m‸undo"),
    ("md", 1, "⟦uno\ndos\ntres⟧", "numerada", "1. ⟦uno\n2. dos\n3. tres⟧"),
    ("md", 0, "⟦uno\ndos\ntres⟧", "numerada", "1. ⟦uno\ndos\ntres⟧"),
    ("md", 0, "⟦uno\n\ndos\n\ntres⟧", "numerada", "1. ⟦uno\n\n2. dos\n\n3. tres⟧"),
    ("md", 0, "⟦- Hola⟧", "numerada", "1. ⟦Hola⟧"),
    ("md", 0, "## Hola m‸undo", "viñetas", "- Hola m‸undo"),
    ("md", 0, "# Hola m‸undo", "casillas", "- [ ] Hola m‸undo"),
    ("md", 0, "> Hola m‸undo", "viñetas", "> - Hola m‸undo"),
    ("md", 0, "> - Hola m‸undo", "viñetas", "> Hola m‸undo"),
    ("md", 0, "> Hola m‸undo", "numerada", "> 1. Hola m‸undo"),
    ("md", 0, "| a | b‸ |\n| - | - |", "viñetas", "| a | b‸ |\n| - | - |"),
    ("md", 0, "```\nHola‸\n```", "numerada", "```\nHola‸\n```"),
    ("md", 0, "```\n- H‸ola\n```", "viñetas", "```\n- H‸ola\n```"),
    ("md", 0, "‸", "casillas", "- [ ] ‸"),
    # Cita
    ("md", 0, "Hola m‸undo", "cita", "> Hola m‸undo"),
    ("md", 0, "> Hola m‸undo", "cita", "Hola m‸undo"),
    ("md", 0, "- Hola m‸undo", "cita", "> - Hola m‸undo"),
    ("md", 0, "## Hola m‸undo", "cita", "> ## Hola m‸undo"),
    ("md", 0, "| a | b‸ |\n| - | - |", "cita", "| a | b‸ |\n| - | - |"),
    ("md", 0, "```\nHola‸\n```", "cita", "```\nHola‸\n```"),
    # Tabla: debajo de la línea, nunca dentro de ella; dentro de una tabla, una fila o una columna.
    ("md", 0, "‸", "tabla", T3_SEL + "\n"),
    ("md", 0, "Antes‸", "tabla", "Antes\n\n" + T3_SEL + "\n"),
    ("md", 0, "Hola mu‸ndo bonito", "tabla", "Hola mundo bonito\n\n" + T3_SEL + "\n"),
    ("md", 0, "Hola ⟦mundo⟧ bonito\nDespués", "tabla", "Hola mundo bonito\n\n" + T3_SEL + "\n\nDespués"),
    ("md", 0, "Hola‸\n\nDespués", "tabla", "Hola\n\n" + T3_SEL + "\n\nDespués"),
    ("md", 0, "Antes\n‸\nDespués", "tabla", "Antes\n\n" + T3_SEL + "\n\nDespués"),
    # Dentro de una tabla, una columna con nombre a la derecha de la del cursor (10/11; antes, al final o una fila)
    ("md", 0, "| a | b |\n| - | - |\n| 1‸ | 2 |", "tabla", "| a   | ⟦Columna 1⟧ | b   |\n| --- | --------- | --- |\n| 1   |           | 2   |"),
    ("md", 0, "| a‸ | b |\n| - | - |\n| 1 | 2 |", "tabla", "| a   | ⟦Columna 1⟧ | b   |\n| --- | --------- | --- |\n| 1   |           | 2   |"),
    ("md", 0, "| a | Columna 1 |\n| - | - |\n| 1 | 2‸ |", "tabla", "| a   | Columna 1 | ⟦Columna 2⟧ |\n| --- | --------- | --------- |\n| 1   | 2         |           |"),
    ("md", 0, T3_SEL + "\n", "tabla",
     "| Columna 1 | ⟦Columna 4⟧ | Columna 2 | Columna 3 |\n| --------- | --------- | --------- | --------- |\n|           |           |           |           |\n"),
    ("md", 0, "‸", "tabla×2",
     "| Columna 1 | ⟦Columna 4⟧ | Columna 2 | Columna 3 |\n| --------- | --------- | --------- | --------- |\n|           |           |           |           |\n"),
    ("md", 0, "```\nHola‸\n```", "tabla", "```\nHola‸\n```"),
    # Alinear: el cursor se queda en su celda
    ("md", 0, "| a | b‸ |\n| - | - |\n| 1 | 2 |", "alinear", "| a   | b‸   |\n| --- | --- |\n| 1   | 2   |"),
    # Intro
    ("md", 0, "- uno‸", "intro", "- uno\n- ‸"),
    ("md", 0, "- uno\n- ‸", "intro", "- uno\n‸"),
    ("md", 0, "1. uno‸", "intro", "1. uno\n2. ‸"),
    ("md", 0, "- [x] uno‸", "intro", "- [x] uno\n- [ ] ‸"),
    ("md", 0, "> uno‸", "intro", "> uno\n>\n> ‸"),
    ("md", 0, "> uno\n> ‸", "intro", "> uno\n‸"),
    ("md", 0, "> - uno‸", "intro", "> - uno\n> - ‸"),
    ("md", 0, "> - uno\n> - ‸", "intro", "> - uno\n> ‸"),
    ("md", 0, "| a | b |\n| - | - |\n| 1 | 2‸ |", "intro", "| a | b |\n| - | - |\n| 1 | 2 |\n| ‸  |   |"),
    ("md", 0, "| a | b |\n| - | - |\n| 1‸ | 2 |", "intro", "| a | b |\n| - | - |\n| 1 | 2 |\n| ‸  |   |"),
    ("md", 0, "| a‸ | b |\n| - | - |\n| 1 | 2 |", "intro", "| a | b |\n| - | - |\n| ‸  |   |\n| 1 | 2 |"),
    ("md", 0, "| a | b |\n| - | - |\n|   |   |‸", "intro", "| a   | b   |\n| --- | --- |\n‸"),
    ("md", 0, "## Hola mu‸ndo", "intro", "## Hola mu\n‸ndo"),
    # Con las líneas juntadas (las de un párrafo se ven seguidas): Intro parte el párrafo con una línea en blanco y
    # Mayús+Intro es el salto de Markdown dentro del párrafo; con las marcas a la vista, el salto de siempre.
    ("md", 0, "uno‸", "intro", "uno\n\n‸"),
    ("md", 0, "uno ‸dos", "intro", "uno \n\n‸dos"),
    ("md", 0, "‸uno", "intro", "\n‸uno"),
    ("md", 0, "uno‸", "mayúsintro", "uno  \n‸"),
    ("md", 1, "uno‸", "intro", "uno\n‸"),
    # Los botones, sobre el párrafo juntado entero: la lista en su primera línea, la cita en todas y el título junta
    # sus líneas en una.
    ("md", 0, "uno\ndo‸s", "viñetas", "- uno\ndo‸s"),
    ("md", 0, "- uno\ndo‸s", "viñetas", "uno\ndo‸s"),
    ("md", 0, "uno\ndo‸s", "cita", "> uno\n> do‸s"),
    ("md", 0, "uno\n  do‸s", "H1", "# uno do‸s"),
    ("md", 0, "> uno\n> do‸s", "H2", "> ## uno do‸s"),
    # En una línea que sigue un párrafo juntado, Intro y Tab son los de su primera línea; Retroceso tras el espacio que
    # se ve y Supr delante de él se llevan el salto con lo escondido de la línea; con algo seleccionado, Intro deja una
    # línea en blanco. Un U+2005 pegado no se lleva los saltos del archivo, y el ajuste de línea junta o separa.
    ("md", 0, "- uno\ndo‸s", "intro", "- uno\ndo\n- ‸s"),
    ("md", 0, "> uno\n> do‸s", "intro", "> uno\n> do\n>\n> ‸s"),
    ("md", 0, "- uno\n- dos\ntr‸es", "tab", "- uno\n\t- dos\n\ttr‸es"),
    ("md", 0, "> uno\n> ‸dos", "retroceso", "> uno‸dos"),
    ("md", 0, "uno‸\n  dos", "supr", "uno‸dos"),
    ("md", 0, "uno ⟦do⟧s", "intro", "uno \n\n‸s"),
    ("md", 0, "uno\ndos‸", "ajuste×2", "uno\ndos‸"),
    ("md", 0, "uno\ndos‸", "pegar:x\u2005y", "uno\ndosx\u2005y‸"),
    ("md", 0, "```\nHola‸\n```", "intro", "```\nHola\n‸\n```"),
    # Retroceso justo detrás de una marca oculta: se va la marca entera; con las marcas a la vista, un carácter.
    ("md", 0, "- ‸uno", "retroceso", "‸uno"),
    ("md", 0, "1. ‸uno", "retroceso", "‸uno"),
    ("md", 0, "- [ ] ‸uno", "retroceso", "‸uno"),
    ("md", 0, "> ‸uno", "retroceso", "‸uno"),
    ("md", 1, "- ‸uno", "retroceso", "-‸uno"),
    # Tab con selección
    ("md", 0, "- uno\n- ⟦dos⟧", "tab", "- uno\n\t- ⟦dos⟧"),
    ("md", 0, "⟦Hola⟧", "tab", "    ⟦Hola⟧"),
    ("md", 0, "## ⟦Hola⟧", "tab", "## ⟦Hola⟧"),
    ("md", 0, "> ⟦Hola⟧", "tab", "> ⟦Hola⟧"),
    ("md", 0, "| ⟦a⟧ |\n| - |", "tab", "| a |\n| - |\n| ‸  |"),
    ("txt", 0, "⟦Hola⟧", "tab", "\t⟦Hola⟧"),
    # Tab sin selección en una lista la baja un nivel; en las numeradas el subnivel empieza en 1 y las de detrás se
    # numeran otra vez, todo en un paso de deshacer (10/11)
    ("md", 0, "- uno\n- d‸os", "tab", "- uno\n\t- d‸os"),
    ("md", 0, "- uno\n\t- dos\n- t‸res", "tab", "- uno\n\t- dos\n\t- t‸res"),
    ("md", 0, "- uno\n  - dos\n- t‸res", "tab", "- uno\n  - dos\n  - t‸res"),
    ("md", 0, "- uno\n\t- d‸os", "tab", "- uno\n\t- d‸os"),
    ("md", 0, "- u‸no", "tab", "- u‸no"),
    ("md", 0, "- [ ] uno\n- [ ] d‸os", "tab", "- [ ] uno\n\t- [ ] d‸os"),
    ("md", 0, "1. uno\n2. d‸os\n3. tres", "tab", "1. uno\n\t1. d‸os\n2. tres"),
    ("md", 0, "1. uno\n   1. dos\n3. t‸res", "tab", "1. uno\n   1. dos\n   2. t‸res"),
    ("md", 0, "1. uno\n\t1. d‸os\n2. tres", "mayústab", "1. uno\n2. d‸os\n3. tres"),
    ("md", 0, "- uno\n\t- ‸dos", "retroceso", "- uno\n- ‸dos"),
    ("md", 0, "1. uno\n2. d‸os\n3. tres", "tab+deshacer", "1. uno\n2. d‸os\n3. tres"),
    ("md", 0, "```\n- d‸os\n```", "tab", "```\n- d‸os\n```"),
    # Tablas (10/11): el botón dentro añade una columna con nombre a la derecha de la del cursor; Intro en cualquier fila
    # vacía sale de la tabla, al principio de una fila añade otra y con «Columna 1» seleccionado no parte la cabecera;
    # Tab y Mayús+Tab van de celda en celda; al salir, la tabla se alinea
    ("md", 0, "| a | b |\n| - | - |\n|   |   ‸|\n| 1 | 2 |\nfin", "intro", "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n‸\nfin"),
    ("md", 0, "| a | b |\n| - | - |\n|   |   ‸|\n| 1 | 2 |", "intro", "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n‸"),
    ("md", 0, "| a | b |\n| - | - |\n‸| 1 | 2 |", "intro", "| a | b |\n| - | - |\n| 1 | 2 |\n| ‸  |   |"),
    ("md", 0, "Hola‸", "tabla+intro", "Hola\n\n| Columna 1 | Columna 2 | Columna 3 |\n| --------- | --------- | --------- |\n"
     "| ‸          |           |           |\n|           |           |           |\n"),
    ("md", 0, "Hola‸", "tabla+intro+intro", "Hola\n\n" + T3 + "\n‸"),
    ("md", 0, "| a‸ | b |\n| - | - |\n| 1 | 2 |", "tab", "| a | ⟦b⟧ |\n| - | - |\n| 1 | 2 |"),
    ("md", 0, "| a | b‸ |\n| - | - |\n| 1 | 2 |", "tab", "| a | b |\n| - | - |\n| ⟦1⟧ | 2 |"),
    ("md", 0, "| a | b |\n| - | - |\n| 1 | 2‸ |", "tab", "| a | b |\n| - | - |\n| 1 | 2 |\n| ‸  |   |"),
    ("md", 0, "| a | b |\n| - | - |\n| 1‸ | 2 |", "mayústab", "| a | ⟦b⟧ |\n| - | - |\n| 1 | 2 |"),
    ("md", 0, "| a | bb |\n| - | - |\n| 1 | 2‸ |\nfin", "izquierda+abajo", "| a   | bb  |\n| --- | --- |\n| 1   | 2   |\nfin‸"),
    ("md", 0, "| a | bb |\n| - | - |\n| 1 | 2‸ |\nfin", "izquierda+abajo+deshacer", "| a | bb |\n| - | - |\n| 1 | 2 |\nfin‸"),
    # De la prueba al azar, semilla 11 (10/11): Retroceso delante de la marca de una cita la dejaba sin barra en pantalla
    # (RichEdit quita la «viñeta» del párrafo); como en las listas, se va la marca
    ("md", 0, "> \n‸>  que tie\t\n> ", "retroceso", "> \n‸que tie\t\n> "),
    # De la prueba al azar, semilla 11 (10/11): romper la valla deja la raya de debajo fuera del código
    ("md", 0, "a\n```‸\n~~~\ntildes\n~~~\ne|---|]| Nombre | E0 |\n---\n!", "retroceso", "a\n``‸\n~~~\ntildes\n~~~\ne|---|]| Nombre | E0 |\n---\n!"),
    ("md", 0, "a\n```‸\nb\n---\n!", "retroceso", "a\n``‸\nb\n---\n!"),
    ("md", 0, "1. uno\n   1. d‸os\n2. tres", "mayústab", "1. uno\n2. d‸os\n3. tres"),
    ("md", 0, "- uno\n  - d‸os", "mayústab", "- uno\n- d‸os"),
    # Intro: en una numerada, los de detrás corren uno; en un elemento vacío de un subnivel, sube un nivel como uno más
    # de esa lista; en el primer nivel, sale de la lista
    ("md", 0, "1. uno‸\n2. dos", "intro", "1. uno\n2. ‸\n3. dos"),
    ("md", 0, "1. uno\n   1. dos\n   2. ‸\n2. tres", "intro", "1. uno\n   1. dos\n2. ‸\n3. tres"),
    ("md", 0, "- uno\n  - dos\n  - ‸", "intro", "- uno\n  - dos\n- ‸"),
    ("md", 0, "- uno\n  - ‸", "intro+intro", "- uno\n‸"),
    # La selección de la línea entera no coge el espacio de la marca nueva
    ("md", 0, "⟦## Hola⟧", "H3", "### ⟦Hola⟧"),
    ("md", 0, "⟦## Hola⟧", "H1", "# ⟦Hola⟧"),
    ("md", 0, "⟦- [ ] Hola⟧", "viñetas", "- ⟦Hola⟧"),
    # De la revisión de entrega
    ("md", 1, "⟦Hola mundo\n⟧X", "H1", "# ⟦Hola mundo\n⟧X"),
    ("md", 0, "⟦Hola mundo\n⟧X", "H1", "# ⟦Hola mundo ⟧X"),
    ("md", 0, "⟦-⟧ abc", "H2", "## ‸abc"),
    ("md", 0, "> > ‸texto", "retroceso", "‸texto"),
    ("md", 0, "Hola ⟦x\n```\nco⟧de\n```", "tabla", "Hola ⟦x\n```\nco⟧de\n```"),
    ("md", 0, "Hola ⟦x\n```\nco⟧de\n```", "enlace", "Hola ⟦x\n```\nco⟧de\n```"),
    ("md", 0, "> - uno\n>   - dos‸", "intro", "> - uno\n>   - dos\n>   - ‸"),
    # Las líneas son iguales: el deshacer guarda el cambio donde empieza la diferencia de textos, dos más abajo. Lo que
    # se comprueba es que todo se ve como repintado entero.
    ("md", 0, "⟦x **b**\nx **b**\nx **b**\n⟧x **b**\nx **b**\nfin", "supr+deshacer",
     "x **b**\nx **b**\n⟦x **b**\nx **b**\nx **b**\n⟧fin"),
    ("txt", 0, "⟦a\n---\nb⟧", "tab", "\t⟦a\n\t---\n\tb⟧"),
    # De la prueba al azar, 09/11: espacios de más tras la marca de lista y una línea vacía en la selección
    ("md", 0, "2.  ⟦uno⟧", "H3", "### ⟦uno⟧"),
    ("md", 1, "-  u⟦no\ndos⟧", "H2", "## u⟦no\n## dos⟧"),
    ("md", 0, "-  u⟦no\ndos⟧", "H2", "## u⟦no dos⟧"),
    ("md", 0, "⟦uno\n\ndos⟧", "cursiva×2", "⟦uno\n\ndos⟧"),
    ("md", 0, "⟦uno\n\ndos⟧", "negrita", "⟦**uno**\n\n**dos**⟧"),
    # RichEdit se queda con el salto final de lo que se sustituye: deshacer un pegado que acababa en salto lo dejaba.
    ("md", 0, "a\n⟦1⟧\nb", "pegar:~~~\n+deshacer", "a\n⟦1⟧\nb"),
    ("md", 0, "a\n⟦1⟧\nb", "pegar:~~~\n+deshacer+rehacer+deshacer", "a\n⟦1⟧\nb"),
    ("md", 0, "a\n⟦1⟧", "pegar:x\n+deshacer", "a\n⟦1⟧"),
    # «~~~~» al principio de una línea abriría un bloque de código y «****» sola sería una raya: ahí el formato va a la
    # palabra de delante, y en una línea vacía no se pone.
    ("md", 0, "a\n‸\nb", "tachado", "a\n‸\nb"),
    ("md", 0, "a\n‸\nb", "negrita", "a\n‸\nb"),
    ("md", 0, "a\n‸Hola mundo", "tachado", "a\n~~⟦Hola⟧~~ mundo"),
    ("md", 0, "Hola ‸", "tachado", "Hola ~~‸~~"),
    ("md", 0, "a\n‸\nb", "tachado×2", "a\n‸\nb"),
    ("md", 0, "‸## Cinco", "tachado×2", "‸## Cinco"),
    ("md", 0, "~~‸~~\ncódigo\n~~~~", "tachado", "~~‸~~\ncódigo\n~~~~"),
    # Al principio de un elemento de lista, delante de su marca oculta, RichEdit quitaba solo la viñeta de la pantalla.
    ("md", 0, "- uno\n‸- dos", "retroceso", "- uno\n‸dos"),
    ("md", 0, "- uno\n‸  - dos", "retroceso", "- uno\n- ‸dos"),
    ("md", 0, "- uno\n‸1. dos", "retroceso", "- uno\n‸dos"),
    # Tras un enlace, el cursor queda entre sus «()» ocultos y el párrafo enseña las marcas: al salir de él el cursor,
    # se vuelven a esconder (de la prueba al azar, 11/11).
    ("md", 0, "Hola ⟦mundo⟧\nfin", "enlace+cursor:16", "Hola [mundo]()\nf‸in"),
    # El cursor de a medias (tras sustituir, entre «~~» y «**» ya ocultos) destapaba el párrafo aunque la selección final
    # no lo necesite (de la matriz por pares, 11/11).
    ("md", 0, "Hola **⟦mundo⟧** bonito", "tachado", "Hola **~~⟦mundo⟧~~** bonito"),
    # Un título cuyo texto empieza por «- »: al quitarlo, o al pasarlo a viñeta, ese «- » es la marca de la lista y lo
    # seleccionado no se corre (de la prueba al azar, 11/11).
    ("md", 0, "###   - Título ⟦tres⟧", "H3", "- Título ⟦tres⟧"),
    ("md", 0, "#  - segundo ⟦ni⟧", "viñetas", " - segundo ⟦ni⟧"),
    # Un título pegado a lo de encima lleva hueco; con una línea en blanco nueva encima, ya no. Sin ajuste de línea no se
    # repintaba la línea de después de lo cambiado (de la prueba al azar, 11/11).
    ("md", 2, "#### Cuatro‸\n##### Cinco", "pegar: dos\n", "#### Cuatro dos\n‸\n##### Cinco"),
    # Sin menú Formato (1.2), los atajos de formato siguen, como desde el teclado («+» separa pasos: «ctrl-B»).
    ("md", 0, "Hola ⟦mundo⟧ bonito", "ctrl-B", "Hola **⟦mundo⟧** bonito"),
    ("md", 0, "Hola ⟦mundo⟧ bonito", "ctrl-I", "Hola *⟦mundo⟧* bonito"),
    ("md", 0, "Hola ⟦mundo⟧", "ctrl-K", "Hola [mundo](‸)"),
    ("md", 0, "Hola m‸undo", "ctrl-1", "# Hola m‸undo"),
    ("md", 0, "Hola m‸undo", "ctrl-2", "## Hola m‸undo"),
    ("md", 0, "Hola m‸undo", "ctrl-3", "### Hola m‸undo"),
    ("md", 0, "| a | b‸ |\n| - | - |\n| 1 | 2 |", "ctrl-mayús-T", "| a   | b‸   |\n| --- | --- |\n| 1   | 2   |"),
]

CONTEXTOS = {
    "texto": "Hola mundo bonito",
    "viñeta": "- Hola mundo bonito",
    "numerada": "1. Hola mundo bonito",
    "casilla": "- [ ] Hola mundo bonito",
    "cita": "> Hola mundo bonito",
    "título": "## Hola mundo bonito",
    "negrita": "Hola **mundo** bonito",
    "tabla": "| Hola | mundo bonito |\n| --- | --- |\n| uno | dos |",
    "código": "```\nHola mundo bonito\n```",
    "vacía": "",
}


def selecciones(linea):
    """(nombre, inicio, fin) dentro de `linea` (la del contexto en que está «mundo»)."""
    i = linea.find("mundo")
    if i < 0:
        return [("vacía", 0, 0)]
    texto = len(linea) - len(linea.lstrip("-0123456789.[] x>#|"))
    return [("cursor", i + 2, i + 2), ("palabra", i, i + 5), ("texto de la línea", texto, len(linea.rstrip(" |"))),
            ("línea entera", 0, len(linea))]


def con_marcas(t, a, b):
    """El texto con la selección a la vista: ⟦…⟧ o ‸, y los saltos como ⏎."""
    s = t[:a] + ("⟦" + t[a:b] + "⟧" if b > a else "‸") + t[b:]
    return s.replace("\r", "⏎")


def leer_marcas(s):
    s = s.replace("\n", "\r")
    if "‸" in s:
        a = s.index("‸")
        return s.replace("‸", ""), a, a
    a = s.index("⟦")
    s = s.replace("⟦", "")
    b = s.index("⟧")
    return s.replace("⟧", ""), a, b


def palabras(t):
    """Las letras, sin marcas, números ni espacios: lo que no puede perderse ni cambiar al dar formato."""
    return re.sub(r"[\W\d_]", "", re.sub(r"\[[ xX]\]", "", t))


class Bloc:
    def __init__(self, escritorio, ext, marcas):
        self.ext, self.marcas = ext, marcas
        # Cada Bloc con su archivo: si otro lo sobrescribe, al guardar pregunta si guardar igual.
        ruta = self.ruta = os.path.join(dir, "botones-%d.%s" % (marcas, ext))
        with open(ruta, "w", encoding="utf-8") as f:
            f.write("x")
        with open(INI, "w") as f:  # noqa: F821
            f.write("ajuste=%d\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=%d\n" % (marcas != 2, marcas == 1))
        si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = "BlocBotones"; self.pi = PI()  # noqa: F821
        k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,  # noqa: F821
                           ctypes.byref(si), ctypes.byref(self.pi))
        self.v = None
        for _ in range(400):
            self.v = next((h for h in ventanas(escritorio, self.pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None)  # noqa: F821
            if self.v:
                break
            time.sleep(0.05)
        self.e = caja(self.v)  # noqa: F821
        enviar(self.v, 0, segundos=30)  # noqa: F821

    def cerrar(self):
        k32.TerminateProcess(self.pi.hProcess, 0)  # noqa: F821
        k32.WaitForSingleObject(self.pi.hProcess, 5000)  # noqa: F821

    def poner(self, t, a, b):
        enviar(self.e, EM_SETSEL, 0, -1)  # noqa: F821
        buf = ctypes.create_unicode_buffer(t)
        enviar(self.e, EM_REPLACESEL, 1, ctypes.addressof(buf))  # noqa: F821
        if texto(self.e) != t:  # noqa: F821
            raise RuntimeError("no se ha podido poner el texto %r" % t)
        enviar(self.e, EM_SETSEL, a, b)  # noqa: F821

    def hacer(self, que):
        if "+" in que:
            for paso in que.split("+")[:-1]:
                self.hacer(paso)
            que = que.split("+")[-1]
        if que.endswith("×2"):
            self.hacer(que[:-2])
            que = que[:-2]
        if que.startswith("cursor:"):
            # Como un clic: el cursor, a esa posición.
            enviar(self.e, EM_SETSEL, int(que[7:]), int(que[7:]))  # noqa: F821
        elif que.startswith("pegar:"):
            # Como otro programa: el texto entra directamente en el control.
            buf = ctypes.create_unicode_buffer(que[6:].replace("\n", "\r"))
            enviar(self.e, EM_REPLACESEL, 1, ctypes.addressof(buf))  # noqa: F821
        elif que in BOTONES:
            enviar(self.v, WM_COMMAND, BOTONES[que])  # noqa: F821
        elif que in MENU:
            enviar(self.v, WM_COMMAND, MENU[que])  # noqa: F821
        elif que.startswith("ctrl-"):
            # Un atajo lo traduce el bucle de mensajes de Bloc mirando las teclas pulsadas: se comparte el estado del
            # teclado con su hilo, se pulsan Ctrl (y Mayús) y la tecla va a su cola, como desde el teclado.
            yo, suyo = k32.GetCurrentThreadId(), u32.GetWindowThreadProcessId(self.e, ctypes.byref(w.DWORD()))  # noqa: F821
            u32.AttachThreadInput(yo, suyo, True)  # noqa: F821
            teclas = (ctypes.c_ubyte * 256)()
            u32.GetKeyboardState(teclas)  # noqa: F821
            for k in [0x11] + ([0x10] if que.startswith("ctrl-mayús-") else []):
                teclas[k] |= 0x80
            u32.SetKeyboardState(teclas)  # noqa: F821
            try:
                u32.PostMessageW(self.e, WM_KEYDOWN, ord(que.rsplit("-", 1)[1]), 0)  # noqa: F821
                time.sleep(0.3)
                enviar(self.e, 0)  # noqa: F821
            finally:
                teclas[0x11] &= 0x7F
                teclas[0x10] &= 0x7F
                u32.SetKeyboardState(teclas)  # noqa: F821
                u32.AttachThreadInput(yo, suyo, False)  # noqa: F821
        elif que in ("mayústab", "mayúsintro"):
            # Bloc mira la tecla Mayúsculas con GetKeyState: se comparte el estado del teclado con su hilo y se pulsa.
            yo, suyo = k32.GetCurrentThreadId(), u32.GetWindowThreadProcessId(self.e, ctypes.byref(w.DWORD()))  # noqa: F821
            u32.AttachThreadInput(yo, suyo, True)  # noqa: F821
            teclas = (ctypes.c_ubyte * 256)()
            u32.GetKeyboardState(teclas); teclas[0x10] |= 0x80; u32.SetKeyboardState(teclas)  # noqa: F821
            try:
                enviar(self.e, WM_KEYDOWN, VK_TAB if que == "mayústab" else VK_RETURN)  # noqa: F821
            finally:
                teclas[0x10] &= 0x7F; u32.SetKeyboardState(teclas)  # noqa: F821
                u32.AttachThreadInput(yo, suyo, False)  # noqa: F821
        else:
            # Solo la tecla: el carácter que la sigue lo quita Bloc de la cola, y mandado aparte sería otro Intro.
            enviar(self.e, WM_KEYDOWN, TECLAS[que])  # noqa: F821
        return texto(self.e), *seleccion(self.e)  # noqa: F821

    def mismo_pintado(self):
        """Lo pintado poco a poco, igual que repintado entero («Mostrar marcas» dos veces). Cada captura, tras dejar a
        Bloc acabar de pintar: el control descongelado repinta un poco después."""
        def pintada():
            enviar(self.v, 0)  # noqa: F821
            time.sleep(0.15)
            return captura(self.e)  # noqa: F821
        antes = pintada()
        enviar(self.v, WM_COMMAND, MARCAS); enviar(self.v, WM_COMMAND, MARCAS)  # noqa: F821
        despues = pintada()
        if igual(antes, despues):  # noqa: F821
            return True
        self.n_capturas = getattr(self, "n_capturas", 0) + 1
        for k, c in enumerate((antes, despues)):
            guardar_png(c, os.path.join(dir, "pintado-%02d-%d.png" % (self.n_capturas, k + 1)))  # noqa: F821
        return False


PARES_SITIOS = dict(CONTEXTOS, **{"sublista": "- Hola\n  - Hola mundo bonito", "numerada en medio": "1. uno\n2. Hola mundo bonito\n3. tres",
                                  "subnumerada": "1. uno\n   1. Hola mundo bonito\n2. tres", "lista en cita": "> - Hola mundo bonito",
                                  "tabla, cuerpo": "| a | b |\n| --- | --- |\n| Hola | mundo bonito |\n| uno | dos |",
                                  "tabla, última fila": "| a | b |\n| --- | --- |\n| Hola | mundo bonito |",
                                  "título 1": "# Hola mundo bonito", "raya debajo": "Hola mundo bonito\n\n---"})
PARES_ACCIONES = list(BOTONES) + ["tab", "mayústab", "intro", "retroceso", "deshacer", "rehacer"]
NO_CAMBIAN_LETRAS = CONMUTAN | {"alinear", "tab", "mayústab"}


VALLA = re.compile(r" {0,3}(`{3,}|~{3,})")
VALLA_TILDES = re.compile(r" {0,3}~{3,}")
MARCA_VACIA = re.compile(r"\s*(>\s*)*(#{1,6}|[-*+]( \[[ xX]\])?|\d{1,9}[.)])\s*$")
SEPARADOR = re.compile(r"\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$")


def es_fila(linea):
    return linea[:len(linea) - len(linea.lstrip(" \t"))].count(" ") < 4 and linea.lstrip(" \t").startswith("|")


def celdas(linea):
    """Las celdas de una fila, como md::celdas."""
    s = linea.strip()
    if s.startswith("|"):
        s = s[1:]
    partes, c, k, cierra = [], "", 0, False
    while k < len(s):
        cierra = False
        if s[k] == "\\" and k + 1 < len(s) and s[k + 1] == "|":
            c += s[k:k + 2]; k += 2
            continue
        if s[k] == "|":
            partes.append(c.strip()); c = ""; cierra = True
        else:
            c += s[k]
        k += 1
    if c.strip() or not cierra:
        partes.append(c.strip())
    return partes


def estructura(t, cursor):
    """Lo que la regla 4 no deja (ver PLAN-FINAL, fase E1), leído como lo lee md.rs: tablas sin cabecera, con filas de
    distinto número de columnas o columnas sin nombre, una tabla dentro de otra, `~~~` que abren código y marcas de
    título o de lista sin texto (salvo en la línea del cursor)."""
    res, lineas, abierta, grupo = set(), t.split("\r"), None, []
    linea_cursor = t.count("\r", 0, cursor)

    def tabla(g):
        if len(g) < 2 or not any(SEPARADOR.match(l) and "-" in l for l in g):
            return
        if not (SEPARADOR.match(g[1]) and "-" in g[1]):
            res.add("tabla sin cabecera")
            return
        cab = celdas(g[0])
        if any(not c for c in cab):
            res.add("columna sin nombre")
        if any(len(celdas(l)) != len(cab) for k, l in enumerate(g) if k != 1):
            res.add("filas con distinto número de columnas")
        if any(SEPARADOR.match(l) and "-" in l for l in g[2:]):
            res.add("tabla dentro de tabla")

    for k, l in enumerate(lineas):
        v = VALLA.match(l)
        if abierta:
            if v and v.group(1)[0] == abierta[0] and len(v.group(1)) >= len(abierta) and not l[v.end():].strip():
                abierta = None
            continue
        if es_fila(l):
            grupo.append(l)
            continue
        tabla(grupo)
        grupo = []
        if v:
            abierta = v.group(1)
        elif MARCA_VACIA.match(l) and l.strip() and k != linea_cursor:
            res.add("marca sin texto")
    tabla(grupo)
    if sum(1 for l in lineas if VALLA_TILDES.match(l)) % 2:
        res.add("~~~ sin cerrar")
    return res


TRIOS = 600


def pares(b, b_marcas, salida, log):
    """Una acción tras otra (BOTONES_PARES=1), en cada sitio y con el cursor, una palabra o varias líneas seleccionadas,
    con las marcas ocultas; y una muestra de tríos y de pares con las marcas a la vista (`b_marcas`). Avisa de lo que no
    debe pasar nunca: P, cambian las letras con dos acciones que no las tocan; U, deshacer no vuelve al texto de antes;
    H, rehacer no vuelve al de después; E, errores.log; R, lo pintado no es igual que repintado entero (una de cada
    veinte, por tiempo); S, la estructura queda rota (ver `estructura`)."""
    import random
    azar = random.Random(1)
    avisados, n = [], 0
    sitios = []
    for ctx, linea in PARES_SITIOS.items():
        doc = "Antes\r\r" + linea.replace("\n", "\r") + "\r\rDespués"
        ini = doc.find("mundo")
        ini = doc.rfind("\r", 0, ini) + 1 if ini >= 0 else len("Antes\r\r")
        fin = doc.find("\r", ini)
        sel = [s for s in selecciones(doc[ini:fin]) if s[0] in ("cursor", "palabra", "vacía")]
        sel.append(("varias líneas", sel[-1][1], len(doc) - 3 - ini))
        sitios.extend((ctx, doc, nombre, ini + a, ini + z) for nombre, a, z in sel)

    def probar(bloc, ctx, doc, nombre, a, z, acciones):
        nonlocal n
        bloc.poner(doc, a, z)
        # La estructura, paso a paso: lo que rompe cada acción de Bloc. Lo que borra quien escribe (Retroceso, Supr,
        # Intro sobre lo seleccionado) puede dejar una columna sin nombre o un título vacío, como en Word; y lo que
        # empieza deshaciendo parte del caso anterior.
        rotas, t1, s1, e1 = set(), doc, a, z
        for x in acciones:
            t0, s0, e0 = t1, s1, e1
            t1, s1, e1 = bloc.hacer(x)
            nuevas = estructura(t1, s1) - estructura(t0, s0)
            if x in ("retroceso", "supr") and s0 != e0 or x == "intro" and "\r" in t0[s0:e0]:
                nuevas = set()
            elif x in ("retroceso", "supr", "intro"):
                nuevas -= {"columna sin nombre", "marca sin texto"}
            rotas |= nuevas
        if acciones[0] in ("deshacer", "rehacer"):
            rotas = set()
        n += 1
        avisos = ""
        if all(x in NO_CAMBIAN_LETRAS for x in acciones) and palabras(t1) != palabras(doc):
            avisos += "P"
        if rotas:
            avisos += "S (%s)" % ", ".join(sorted(rotas))
        if "deshacer" not in acciones and "rehacer" not in acciones and t1 != doc:
            pasos = 0
            while pasos < len(acciones) and texto(bloc.e) != doc:  # noqa: F821
                bloc.hacer("deshacer"); pasos += 1
            if texto(bloc.e) != doc:  # noqa: F821
                avisos += "U"
            else:
                for _ in range(pasos):
                    bloc.hacer("rehacer")
                if texto(bloc.e) != t1:  # noqa: F821
                    avisos += "H"
        if os.path.exists(log):
            with open(log, encoding="utf-8", errors="replace") as f:
                avisos += "E " + f.read()[-200:]
            os.remove(log)
        if azar.random() < 0.05:
            bloc.poner(doc, a, z)
            bloc.hacer("+".join(acciones))
            if not bloc.mismo_pintado():
                avisos += "R"
        if avisos:
            avisados.append("| %s%s | %s | %s | `%s` | `%s` | %s |" % (
                ctx, " (marcas)" if bloc.marcas else "", nombre, " → ".join(acciones), con_marcas(doc, a, z), con_marcas(t1, s1, e1), avisos))

    for ctx, doc, nombre, a, z in sitios:
        for x in PARES_ACCIONES:
            for y in PARES_ACCIONES:
                probar(b, ctx, doc, nombre, a, z, [x, y])
                if azar.random() < 0.1:
                    probar(b_marcas, ctx, doc, nombre, a, z, [x, y])
    for _ in range(TRIOS):
        ctx, doc, nombre, a, z = azar.choice(sitios)
        probar(b, ctx, doc, nombre, a, z, [azar.choice(PARES_ACCIONES) for _ in range(3)])
    salida.append("\n## Pares y tríos de acciones\n\n%d combinaciones, %d con aviso. P, cambian las letras; U, deshacer no "
                  "vuelve; H, rehacer no vuelve; E, errores.log; R, pintado distinto; S, estructura rota.\n" % (n, len(avisados)))
    salida.append("| sitio | selección | acciones | antes | después | avisos |\n|---|---|---|---|---|---|")
    salida.extend(avisados)
    print("pares: %d, con aviso %d" % (n, len(avisados)), flush=True)


def trabajo():
    escritorio = u32.CreateDesktopW("BlocBotones", None, None, 0, 0x10000000, None)  # noqa: F821
    u32.SetThreadDesktop(escritorio)  # noqa: F821
    log = os.path.join(dir, "Bloc", "errores.log")
    salida = ["# Botones de Bloc, caso a caso\n"]
    fallos = 0
    blocs = {}

    def bloc(ext, marcas):
        if (ext, marcas) not in blocs:
            blocs[(ext, marcas)] = Bloc(escritorio, ext, marcas)
        return blocs[(ext, marcas)]

    try:
        salida.append("## Casos escritos a mano\n")
        for ext, marcas, antes, que, despues in CASOS:
            # BOTONES_FILTRO=texto¦otro: solo los casos cuyo texto de antes lleve alguno.
            if not any(f in antes.replace("\n", "⏎") for f in os.environ.get("BOTONES_FILTRO", "").split("¦")):
                continue
            b = bloc(ext, marcas)
            t, a, z = leer_marcas(antes)
            b.poner(t, a, z)
            r = b.hacer(que)
            hay = con_marcas(*r)
            # Un U+2005 de verdad se lee del control como un salto blando: lo que cuenta es lo que se guarda.
            u2005 = "\u2005" in que
            esperado = con_marcas(*leer_marcas(despues.replace("\u2005", "\n") if u2005 else despues)) if despues is not None else hay
            pintado = ext == "txt" or marcas == 1 or b.mismo_pintado()
            bien = hay == esperado and pintado
            if u2005:
                fecha = os.path.getmtime(b.ruta)
                enviar(b.v, WM_COMMAND, GUARDAR)  # noqa: F821
                for _ in range(100):
                    if os.path.getmtime(b.ruta) != fecha:
                        break
                    time.sleep(0.05)
                time.sleep(0.3)
                with open(b.ruta, encoding="utf-8") as f:
                    guardado = f.read().replace("\r\n", "\n").replace("\r", "\n")
                if guardado != leer_marcas(despues)[0].replace("\r", "\n"):
                    bien = False
                    hay += "   (guardado %r)" % guardado
            fallos += not bien
            linea = "%s %s · %s · %s → %s" % ("ok   " if bien else "FALLO", ext, que, con_marcas(t, a, z), hay)
            if hay != esperado:
                linea += "   (esperado %s)" % esperado
            if not pintado:
                linea += "   (lo pintado no es igual que repintado entero)"
            print(linea, flush=True)
            salida.append("- " + linea)
            # Un U+2005 de verdad deja de juntar las líneas en ese Bloc: los casos siguientes, en uno nuevo.
            if "\u2005" in que:
                b.cerrar()
                del blocs[(ext, marcas)]
        salida.append("\n## Todas las combinaciones\n")
        salida.append("Avisos: P, cambia el texto (sin marcas); D, pulsado dos veces no vuelve; S, la selección no sigue.\n")
        # BOTONES_SOLO_CASOS=1: solo los casos escritos a mano (un minuto, en vez de diez).
        for ext, marcas in () if os.environ.get("BOTONES_SOLO_CASOS") else (("md", 0), ("md", 1), ("txt", 0)):
            b = bloc(ext, marcas)
            salida.append("\n### %s, marcas %s\n\n| sitio | selección | qué | antes | después | avisos |\n|---|---|---|---|---|---|" % (
                ext, "a la vista" if marcas else "ocultas"))
            for ctx, linea in CONTEXTOS.items():
                doc = "Antes\r\r" + linea.replace("\n", "\r") + "\r\rDespués"
                ini = doc.find("mundo")
                ini = doc.rfind("\r", 0, ini) + 1 if ini >= 0 else len("Antes\r\r")
                fin = doc.find("\r", ini)
                for nombre, a, z in selecciones(doc[ini:fin]):
                    for que in list(BOTONES) + list(TECLAS):
                        if ext == "txt" and que in BOTONES and que not in ("negrita", "H1", "viñetas", "tabla"):
                            continue
                        b.poner(doc, ini + a, ini + z)
                        t1, s1, e1 = b.hacer(que)
                        avisos = ""
                        if que not in ("enlace", "tabla", "intro", "retroceso", "tab", "supr") and palabras(t1) != palabras(doc):
                            avisos += "P"
                        if que in CONMUTAN and ext == "md":
                            enviar(b.e, EM_SETSEL, s1, e1)  # noqa: F821
                            t2, _, _ = b.hacer(que)
                            if t2 != doc:
                                avisos += "D"
                        if que in BOTONES and z > a and palabras(t1[s1:e1]) != palabras(doc[ini + a:ini + z]):
                            avisos += "S"
                        if os.path.exists(log):
                            with open(log, encoding="utf-8", errors="replace") as f:
                                avisos += " errores.log: " + f.read()[-200:]
                            os.remove(log)
                        recorte = lambda t: t[len("Antes\r\r"):len(t) - len("\r\rDespués")] if t.startswith("Antes\r\r") and t.endswith("\r\rDespués") else t  # noqa: E731
                        d = len("Antes\r\r") if t1.startswith("Antes\r\r") else 0
                        salida.append("| %s | %s | %s | `%s` | `%s` | %s |" % (
                            ctx, nombre, que, con_marcas(recorte(doc), a + ini - 7, z + ini - 7),
                            con_marcas(recorte(t1), s1 - d, e1 - d), avisos))
        if os.environ.get("BOTONES_PARES"):
            pares(bloc("md", 0), bloc("md", 1), salida, log)
    finally:
        for b in blocs.values():
            b.cerrar()
    with open(informe, "w", encoding="utf-8") as f:
        f.write("\n".join(salida) + "\n")
    print("%d fallos de %d casos escritos a mano; informe en %s" % (fallos, len(CASOS), informe))


h = threading.Thread(target=trabajo)
h.start(); h.join()
