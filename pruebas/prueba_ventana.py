"""Prueba Bloc.exe en un escritorio de Windows aparte e invisible: no sale nada en pantalla.
Uso: python prueba_ventana.py <Bloc.exe> <carpeta de trabajo>
Mide arranque y memoria (texto, Markdown y un archivo grande), comprueba que abre el archivo, que
deshace, y que al cerrar con cambios pregunta y «Sí» guarda respetando codificación y saltos.
El cuadro de texto es un RichEdit: WM_GETTEXT da los saltos como \r\n, pero sus posiciones (EM_SETSEL,
EM_GETSEL) cuentan cada salto como un solo carácter."""
import ctypes, ctypes.wintypes as w, os, sys, threading, time

k32, u32 = ctypes.windll.kernel32, ctypes.windll.user32
u32.CreateDesktopW.restype = w.HANDLE
u32.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u32.SendMessageW.restype = w.LPARAM
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
WM_GETTEXT, WM_GETTEXTLENGTH, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_UNDO = 0x0D, 0x0E, 0x102, 0x10, 0x111, 0x304


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


class MEM(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("PageFaultCount", w.DWORD)] + [(n, ctypes.c_size_t) for n in (
        "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
        "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]


ESCRITORIO = "BlocPruebas"
escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
os.environ["BLOC_SIN_INSTALAR"] = "1"   # el Bloc.exe de prueba se usa donde está, sin instalarse
# Un registro vacío: con el inicio con Windows del usuario puesto, «/reinicio» no abriría nada.
os.environ["BLOC_REGISTRO"] = r"Software\BlocPruebasVentana"
fallos = 0
# Una configuración propia, para no tocar la del usuario aunque esté usando Bloc a la vez.
os.environ["APPDATA"] = sys.argv[2]   # errores.log va a la carpeta de la prueba, nunca a la del usuario
INI = os.environ["BLOC_INI"] = os.path.join(sys.argv[2], "bloc.ini")
with open(INI, "w") as f:   # las pruebas cuentan con el ajuste de línea puesto
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")


def ok(bien, que):
    global fallos
    print(("ok     " if bien else "FALLO  ") + que)
    fallos += 0 if bien else 1


def lanzar(exe, archivo):
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO
    pi = PI()
    t0 = time.perf_counter()
    if not k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, archivo)), None, None,
                              False, 0, None, None, ctypes.byref(si), ctypes.byref(pi)):
        sys.exit("CreateProcess: %d" % k32.GetLastError())
    u32.WaitForInputIdle(pi.hProcess, 15000)
    return pi, (time.perf_counter() - t0) * 1000


def ventanas(pid):
    res = []
    def cb(h, _):
        p = w.DWORD(); u32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and u32.IsWindowVisible(h):
            res.append(h)
        return True
    u32.EnumDesktopWindows(escritorio, ENUM(cb), 0)
    return res


def texto(h):
    n = u32.SendMessageW(h, WM_GETTEXTLENGTH, 0, 0)
    b = ctypes.create_unicode_buffer(n + 1)
    u32.SendMessageW(h, WM_GETTEXT, n + 1, ctypes.addressof(b))
    # Los saltos blandos de un .md van en el control como U+2005 (md::JUNTO): el texto del archivo lleva el salto.
    return b.value.replace(" ", "\r\n")


def clase(h):
    b = ctypes.create_unicode_buffer(256); u32.GetClassNameW(h, b, 256); return b.value


def esperar(cond, segundos=8):
    fin = time.time() + segundos
    while time.time() < fin:
        r = cond()
        if r:
            return r
        time.sleep(0.05)
    return None


def principal(pid, nombre):
    return esperar(lambda: next((h for h in ventanas(pid) if texto(h).endswith(nombre + " - Bloc")), None))


def caja(h):
    res = []
    def cb(x, _):
        if clase(x) == "RICHEDIT50W":
            res.append(x)
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res[0] if res else None


def memoria(pi):
    m = MEM(); m.cb = ctypes.sizeof(MEM)
    k32.K32GetProcessMemoryInfo(pi.hProcess, ctypes.byref(m), m.cb)
    return m.WorkingSetSize / 1048576, m.PrivateUsage / 1048576


def terminar(pi, segundos=6):
    acabado = k32.WaitForSingleObject(pi.hProcess, int(segundos * 1000)) == 0
    if not acabado:
        k32.TerminateProcess(pi.hProcess, 1)
    return acabado


def editar_y_guardar(exe, ruta, original, tecla):
    nombre = os.path.basename(ruta)
    pi, ms = lanzar(exe, ruta)
    v = principal(pi.dwProcessId, nombre)
    ok(v is not None, "%s: abre la ventana (%.0f ms)" % (nombre, ms))
    if not v:
        terminar(pi, 0); return ms
    e = caja(v)
    ok(texto(e) == original.replace("\r\n", "\n").replace("\n", "\r\n"), nombre + ": enseña el texto")
    u32.PostMessageW(e, WM_CHAR, ord("x"), 0)
    ok(esperar(lambda: texto(v).startswith("*")) is not None and texto(e).startswith("x"), nombre + ": escribe y marca el cambio")
    u32.SendMessageW(e, WM_UNDO, 0, 0)
    ok(not texto(e).startswith("x"), nombre + ": deshace")
    u32.PostMessageW(e, WM_CHAR, ord(tecla), 0)
    esperar(lambda: texto(e).startswith(tecla))
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    d = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"), None))
    ok(d is not None, nombre + ": al cerrar con cambios pregunta")
    if d:
        u32.PostMessageW(d, WM_COMMAND, 6, 0)   # IDYES
    ok(terminar(pi), nombre + ": con «Sí» guarda y se cierra")
    return ms


def abrir_y_cerrar(exe, ruta, que):
    pi, ms = lanzar(exe, ruta)
    v = principal(pi.dwProcessId, os.path.basename(ruta))
    time.sleep(1.5)
    ws, priv = memoria(pi)
    if v:
        u32.PostMessageW(v, WM_CLOSE, 0, 0)
    ok(v is not None and terminar(pi), "%s: %.0f ms hasta poder escribir, %.0f MB de memoria" % (que, ms, ws))


exe, dir = sys.argv[1], sys.argv[2]
utf = os.path.join(dir, "utf8-lf.txt")
ansi = os.path.join(dir, "ansi.txt")
with open(utf, "wb") as f:
    f.write("línea uno\nlínea dos\n".encode("utf-8"))
with open(ansi, "wb") as f:
    f.write("canción\r\n".encode("cp1252"))
editar_y_guardar(exe, utf, "línea uno\nlínea dos\n", "y")
with open(utf, "rb") as f:
    ok(f.read() == "ylínea uno\nlínea dos\n".encode("utf-8"), "UTF-8 sin BOM y LF se conservan al guardar")
editar_y_guardar(exe, ansi, "canción\r\n", "z")
with open(ansi, "rb") as f:
    ok(f.read() == "zcanción\r\n".encode("cp1252"), "ANSI se conserva al guardar")

md = os.path.join(dir, "notas.md")
with open(md, "w", encoding="utf-8") as f:
    f.write("# Notas\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- uno\n- dos\n" * 40)
grande = os.path.join(dir, "grande.txt")
with open(grande, "w", encoding="utf-8") as f:
    f.write("".join("Línea %d de un archivo grande con texto de relleno\n" % i for i in range(100000)))
abrir_y_cerrar(exe, utf, "texto")
abrir_y_cerrar(exe, md, "Markdown con formato")
abrir_y_cerrar(exe, grande, "archivo de %.1f MB" % (os.path.getsize(grande) / 1048576))
abrir_y_cerrar(exe, os.path.join(dir, "no-existe.txt"), "archivo nuevo")
ok(not [n for n in os.listdir(dir) if n.endswith(".bloc-tmp")], "no deja temporales")

# --- Órdenes de los menús, como si se pulsaran ---
u32.GetMenu.restype = u32.GetSubMenu.restype = w.HMENU
u32.GetMenu.argtypes = [w.HWND]
u32.GetSubMenu.argtypes = [w.HMENU, ctypes.c_int]
u32.GetMenuItemCount.argtypes = [w.HMENU]
u32.GetMenuStringW.argtypes = [w.HMENU, w.UINT, w.LPWSTR, ctypes.c_int, w.UINT]
u32.GetMenuItemID.argtypes = [w.HMENU, ctypes.c_int]
u32.GetMenuItemID.restype = w.UINT
u32.GetParent.argtypes = [w.HWND]
u32.GetParent.restype = w.HWND
EM_GETSEL, EM_SETSEL, WM_SETTEXT, WM_KEYDOWN, IDOK, IDYES, IDNO = 0xB0, 0xB1, 0x0C, 0x100, 1, 6, 7


def menu_id(v, *ruta):
    m = u32.GetMenu(v)
    for k, nombre in enumerate(ruta):
        for i in range(u32.GetMenuItemCount(m)):
            b = ctypes.create_unicode_buffer(256)
            u32.GetMenuStringW(m, i, b, 256, 0x400)
            if b.value.replace("&", "").split("\t")[0].startswith(nombre):
                if k == len(ruta) - 1:
                    return u32.GetMenuItemID(m, i)
                m = u32.GetSubMenu(m, i)
                break
        else:
            raise KeyError(nombre)


def orden(v, *ruta, esperar_fin=True):
    (u32.SendMessageW if esperar_fin else u32.PostMessageW)(v, WM_COMMAND, menu_id(v, *ruta), 0)


def hijos(h):
    res = []
    def cb(x, _):
        res.append(x)
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res


def boton(h, rotulo):
    return next(x for x in hijos(h) if "BUTTON" in clase(x).upper() and texto(x).replace("&", "") == rotulo)


u32.GetDlgCtrlID.argtypes = [w.HWND]


def responder(d, rotulo):
    """Pulsa un botón de un aviso de Windows (el de un solo botón no se cierra con IDOK a secas)."""
    b = boton(d, rotulo)
    u32.PostMessageW(d, WM_COMMAND, u32.GetDlgCtrlID(b), b)


def pulsar(b, esperar_fin=True):
    (u32.SendMessageW if esperar_fin else u32.PostMessageW)(u32.GetParent(b), WM_COMMAND, u32.GetDlgCtrlID(b), b)


def cr(t, i):
    """De una posición en el texto con \r\n a una del cuadro de texto."""
    return i - t[:i].count("\r\n")


def seleccion(e):
    r = u32.SendMessageW(e, EM_GETSEL, 0, 0)
    return r & 0xFFFF, (r >> 16) & 0xFFFF


def teclear(e, s):
    for ch in s:
        u32.SendMessageW(e, WM_CHAR, ord(ch), 0)


def dialogo(pid, titulo=None):
    return esperar(lambda: next((h for h in ventanas(pid) if (titulo is None and clase(h) == "#32770")
                                 or (titulo is not None and texto(h) == titulo)), None), 5)


def abrir(ruta):
    pi, _ = lanzar(exe, ruta)
    v = principal(pi.dwProcessId, os.path.basename(ruta))
    return pi, v, caja(v) if v else None


def cerrar_sin_guardar(pi, v):
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    d = dialogo(pi.dwProcessId)
    if d:
        u32.PostMessageW(d, WM_COMMAND, IDNO, 0)
    return terminar(pi)


original = "Gato gato GATO perro\r\nlínea dos\r\nlínea tres"
ruta = os.path.join(dir, "ordenes.txt")
with open(ruta, "w", encoding="utf-8", newline="") as f:
    f.write(original)
pi, v, e = abrir(ruta)
teclear(e, "hola mundo ")
ok(texto(e) == "hola mundo " + original, "menús: escribe")
for _ in range(3):
    orden(v, "Edición", "Deshacer")
ok(texto(e) == original, "menús: tres «Deshacer» quitan «hola», « mundo» y « »")
for _ in range(3):
    orden(v, "Edición", "Rehacer")
ok(texto(e) == "hola mundo " + original, "menús: tres «Rehacer» lo devuelven")
for _ in range(3):
    orden(v, "Edición", "Deshacer")

# --- Buscar y reemplazar: el panel de debajo de la barra ---
class GUI(ctypes.Structure):
    _fields_ = [("cbSize", w.DWORD), ("flags", w.DWORD)] + [(n, w.HWND) for n in (
        "hwndActive", "hwndFocus", "hwndCapture", "hwndMenuOwner", "hwndMoveSize", "hwndCaret")] + [("rcCaret", w.RECT)]


def panel_de(v):
    return next((x for x in hijos(v) if clase(x) == "BlocBuscar"), None)


def alto_de(h):
    r = w.RECT()
    u32.GetWindowRect(h, ctypes.byref(r))
    return r.bottom - r.top


def casilla(p, n):
    return next(x for x in hijos(p) if u32.GetDlgCtrlID(x) == n)   # 10 la de buscar, 12 la de reemplazar


def poner(c, s):
    b = ctypes.create_unicode_buffer(s)   # en una variable: si no, se libera antes de usarlo
    u32.SendMessageW(c, WM_SETTEXT, 0, ctypes.addressof(b))


def accion(p, n):
    """1 la fila de reemplazar, 2 anterior, 3 siguiente, 4 mayúsculas, 5 cerrar, 6 reemplazar, 7 reemplazar todo."""
    u32.SendMessageW(p, WM_COMMAND, n, 0)


def foco(pi):
    """Desde otro escritorio, Windows no deja leerlo: se pregunta desde un hilo puesto en el de la prueba."""
    g = GUI()
    g.cbSize = ctypes.sizeof(GUI)
    def leer():
        u32.SetThreadDesktop(escritorio)
        u32.GetGUIThreadInfo(pi.dwThreadId, ctypes.byref(g))
    h = threading.Thread(target=leer)
    h.start()
    h.join()
    return g.hwndFocus


def tiempo_cpu(pi):
    t = [w.FILETIME() for _ in range(4)]
    k32.GetProcessTimes(pi.hProcess, *(ctypes.byref(x) for x in t))
    return sum(x.dwHighDateTime << 32 | x.dwLowDateTime for x in t[2:]) / 10000


animaciones = w.BOOL()
u32.SystemParametersInfoW(0x1042, 0, ctypes.byref(animaciones), 0)   # SPI_GETCLIENTAREAANIMATION
u32.SendMessageW(e, EM_SETSEL, 5, 9)
orden(v, "Edición", "Buscar")
p = panel_de(v)
altos = []
t0 = time.perf_counter()
while p and time.perf_counter() < t0 + 0.5:
    altos.append((time.perf_counter() - t0, alto_de(p)))
    time.sleep(0.002)
pasos = [(t, a) for i, (t, a) in enumerate(altos) if i == 0 or a != altos[i - 1][1]]
ok(p is not None and u32.IsWindowVisible(p) and altos[-1][1] == 36, "buscar: «Buscar» abre el panel, de una fila (%s px)" % (altos[-1][1:],))
if animaciones:
    # Ocho pasos que frenan al final, uno por cada aviso del reloj: si se mira tarde, alguno no se llega a ver.
    curva = [0] + [round(36 * (1 - (1 - k / 8) ** 2)) for k in range(1, 9)]
    alturas = [a for _, a in pasos]
    ok(alturas == sorted(alturas) and set(alturas) <= set(curva) and len(alturas) >= 6,
       "buscar: se despliega en ocho pasos que frenan al final (%.0f ms, vistos %s)" % (pasos[-1][0] * 1000, alturas))
cpu = tiempo_cpu(pi)
time.sleep(1)
cpu = tiempo_cpu(pi) - cpu
ok(cpu < 20, "buscar: abierto y quieto no gasta (%.0f ms de CPU en 1 s)" % cpu)
que = casilla(p, 10)
ok(texto(que) == "gato" and texto(p) == "2 de 3", "buscar: lo seleccionado pasa a la casilla y cuenta «%s»" % texto(p))
ok(foco(pi) == que, "buscar: el cursor va a la casilla (%s, casilla %s, texto %s)" % (foco(pi), que, e))
poner(que, "línea")
ok(seleccion(e) == (21, 26) and texto(p) == "1 de 2", "buscar: al escribir busca al momento desde el cursor (%s, «%s»)" % (seleccion(e), texto(p)))
accion(p, 3)
ok(seleccion(e) == (31, 36) and texto(p) == "2 de 2", "buscar: «Siguiente» va a la segunda")
accion(p, 3)
ok(seleccion(e) == (21, 26), "buscar: y después da la vuelta a la primera")
accion(p, 2)
ok(seleccion(e) == (31, 36), "buscar: «Anterior» da la vuelta hacia atrás")
accion(p, 4)
poner(que, "Gato")
ok(seleccion(e) == (0, 4) and texto(p) == "1 de 1", "buscar: distinguiendo mayúsculas, «Gato» sale una vez («%s»)" % texto(p))
accion(p, 4)
ok(texto(p) == "1 de 3", "buscar: sin distinguirlas, tres («%s»)" % texto(p))
poner(que, "nada")
ok(texto(p) == "Sin resultados", "buscar: «Sin resultados» si no hay ninguna")
orden(v, "Edición", "Reemplazar")
ok(esperar(lambda: alto_de(p) == 72, 2) is not None, "buscar: «Reemplazar» despliega la segunda fila")
por = casilla(p, 12)
poner(que, "gato")
poner(por, "can")
accion(p, 7)
ok(texto(e) == original.replace("Gato gato GATO", "can can can"), "buscar: «Reemplazar todo» cambia las tres sin distinguir mayúsculas")
orden(v, "Edición", "Deshacer")
ok(texto(e) == original, "buscar: se deshace de una vez")
u32.SendMessageW(e, EM_SETSEL, 0, 0)
poner(que, "gato")   # selecciona la primera
accion(p, 6)
ok(texto(e) == original.replace("Gato", "can", 1) and seleccion(e) == (4, 8), "buscar: «Reemplazar» cambia esa y va a la siguiente (%s)" % (seleccion(e),))
orden(v, "Edición", "Deshacer")
accion(p, 1)
ok(esperar(lambda: alto_de(p) == 36, 2) is not None, "buscar: el botón de la izquierda pliega la fila de reemplazar")
u32.PostMessageW(que, WM_KEYDOWN, 0x1B, 0)   # Esc
ok(esperar(lambda: not u32.IsWindowVisible(p), 2) is not None, "buscar: Esc lo cierra")
ok(foco(pi) == e, "buscar: y el cursor vuelve al texto (%s)" % foco(pi))
u32.SendMessageW(e, EM_SETSEL, 0, 0)
orden(v, "Edición", "Buscar siguiente")
ok(seleccion(e) == (0, 4) and not u32.IsWindowVisible(p), "buscar: F3 busca otra vez con el panel cerrado")
u32.SendMessageW(e, EM_SETSEL, 0, 0)

orden(v, "Ver", "Ajuste de línea")
orden(v, "Ver", "Ajuste de línea")
ok(texto(e) == original, "ver: cambiar el ajuste de línea no toca el texto")
teclear(e, "z")
orden(v, "Edición", "Deshacer")
ok(texto(e) == original, "ver: después del ajuste, deshacer sigue cuadrando")
for _ in range(3):
    orden(v, "Ver", "Acercar")
orden(v, "Ver", "Tamaño normal")
orden(v, "Edición", "Ir a la línea", esperar_fin=False)
d = dialogo(pi.dwProcessId, "Ir a la línea")
if d:
    c = next(x for x in hijos(d) if "EDIT" in clase(x).upper())
    poner(c, "3")
    pulsar(boton(d, "Aceptar"), esperar_fin=False)
    esperar(lambda: not u32.IsWindowVisible(d))
ok(d is not None and seleccion(e) == (31, 31), "ir a la línea 3")
ok(cerrar_sin_guardar(pi, v), "cerrar con «No» no guarda y se cierra")
with open(ruta, encoding="utf-8", newline="") as f:
    ok(f.read() == original, "el archivo sigue como estaba")

original = "título\r\n\r\nuno\r\ndos\r\n"   # el título, su propio párrafo: si no, se junta con las líneas de debajo
ruta = os.path.join(dir, "formato.md")
with open(ruta, "w", encoding="utf-8", newline="") as f:
    f.write(original)
pi, v, e = abrir(ruta)
# Todo el formato está en la barra de botones: no hay menú Formato.
barra = u32.GetMenu(v)
nombres = []
for i in range(u32.GetMenuItemCount(barra)):
    b = ctypes.create_unicode_buffer(64)
    u32.GetMenuStringW(barra, i, b, 64, 0x400)
    nombres.append(b.value.replace("&", ""))
ok(nombres == ["Archivo", "Edición", "Ver"], "la barra de menús de un .md es Archivo, Edición y Ver: %s" % nombres)
NEGRITA, TITULO1, NUMERADA, CASILLAS, ALINEAR = 1030, 1035, 1039, 1040, 1043


def boton_formato(v, id):
    """Lo que manda el botón de la barra al pulsarlo."""
    u32.SendMessageW(v, WM_COMMAND, id, 0)


u32.SendMessageW(e, EM_SETSEL, 0, 0)
boton_formato(v, TITULO1)
ok(texto(e).startswith("# título\r\n"), "formato: Título 1")
boton_formato(v, TITULO1)
ok(texto(e) == original, "formato: Título 1 otra vez lo quita")
t = texto(e)
u32.SendMessageW(e, EM_SETSEL, cr(t, t.index("uno")), cr(t, t.index("dos") + 3))
boton_formato(v, NUMERADA)
# «uno» y «dos», seguidas, son un párrafo (como en la vista previa): un solo elemento.
ok(texto(e) == "título\r\n\r\n1. uno\r\ndos\r\n", "formato: lista numerada")
boton_formato(v, CASILLAS)
ok(texto(e) == "título\r\n\r\n- [ ] uno\r\ndos\r\n", "formato: lista de casillas")
t = texto(e)
u32.SendMessageW(e, EM_SETSEL, cr(t, t.index("uno")), cr(t, t.index("uno")) + 3)
boton_formato(v, NEGRITA)
ok("**uno**" in texto(e), "formato: negrita")
boton_formato(v, NEGRITA)
ok("**" not in texto(e), "formato: negrita otra vez la quita")
t = texto(e)
fin_dos = cr(t, t.index("dos") + 3)
u32.SendMessageW(e, EM_SETSEL, fin_dos, fin_dos)
u32.SendMessageW(e, WM_KEYDOWN, 0x0D, 0)
ok(texto(e) == "título\r\n\r\n- [ ] uno\r\ndos\r\n- [ ] \r\n", "formato: Intro en una lista sigue la lista")
u32.SendMessageW(e, WM_KEYDOWN, 0x0D, 0)
ok(texto(e) == "título\r\n\r\n- [ ] uno\r\ndos\r\n\r\n", "formato: Intro en un elemento vacío termina la lista")
fin = cr(texto(e), len(texto(e)))
u32.SendMessageW(e, EM_SETSEL, fin, fin)
teclear(e, "| a | importe largo |\r| - | - |\r| texto | 5 |")
boton_formato(v, ALINEAR)
filas = [l for l in texto(e).split("\r\n") if l.startswith("|")]
ok(len(filas) == 3 and len({len(l) for l in filas}) == 1, "formato: alinear las tablas deja las filas iguales")
for _ in range(60):
    antes = texto(e)
    orden(v, "Edición", "Deshacer")
    if texto(e) == antes:
        break
ok(texto(e) == original, "formato: deshaciendo todo se vuelve exactamente al original")
ok(cerrar_sin_guardar(pi, v), "formato: se cierra bien")

ruta = os.path.join(dir, "solo-lectura.txt")
with open(ruta, "w", encoding="utf-8") as f:
    f.write("abc")
os.chmod(ruta, 0o444)
pi, v, e = abrir(ruta)
teclear(e, "x")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
u32.PostMessageW(dialogo(pi.dwProcessId), WM_COMMAND, IDYES, 0)
time.sleep(0.5)
aviso = dialogo(pi.dwProcessId)
ok(aviso is not None and any("No se ha podido guardar" in texto(x) for x in hijos(aviso)), "solo lectura: avisa de que no puede guardar")
if aviso:
    responder(aviso, "Aceptar")
esperar(lambda: aviso and not u32.IsWindow(aviso))
ok(k32.WaitForSingleObject(pi.hProcess, 0) != 0 and texto(e) == "xabc", "solo lectura: la ventana sigue abierta con el texto")
ok(cerrar_sin_guardar(pi, v), "solo lectura: luego se cierra con «No»")
os.chmod(ruta, 0o666)
with open(ruta, encoding="utf-8") as f:
    ok(f.read() == "abc", "solo lectura: el archivo no se ha tocado")

ruta = os.path.join(dir, "cambiado-fuera.txt")
with open(ruta, "w", encoding="utf-8") as f:
    f.write("version 1")
pi, v, e = abrir(ruta)
time.sleep(1.1)
with open(ruta, "w", encoding="utf-8") as f:
    f.write("version 2 de otro programa")
teclear(e, "x")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
u32.PostMessageW(dialogo(pi.dwProcessId), WM_COMMAND, IDYES, 0)
time.sleep(0.5)
aviso = dialogo(pi.dwProcessId)
ok(aviso is not None and any("cambiado fuera de Bloc" in texto(x) for x in hijos(aviso)), "cambiado fuera: avisa antes de machacarlo")
if aviso:
    responder(aviso, "No")
esperar(lambda: aviso and not u32.IsWindow(aviso))
ok(cerrar_sin_guardar(pi, v), "cambiado fuera: con «No» no guarda")
with open(ruta, encoding="utf-8") as f:
    ok(f.read() == "version 2 de otro programa", "cambiado fuera: lo del otro programa sigue ahí")

# Abierto cuando no existía (como con el NAS desconectado) y aparece antes de guardar.
ruta = os.path.join(dir, "aparece.txt")
if os.path.exists(ruta):
    os.remove(ruta)
pi, v, e = abrir(ruta)
with open(ruta, "w", encoding="utf-8") as f:
    f.write("el de verdad")
teclear(e, "x")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
u32.PostMessageW(dialogo(pi.dwProcessId), WM_COMMAND, IDYES, 0)
time.sleep(0.5)
aviso = dialogo(pi.dwProcessId)
ok(aviso is not None and any("no existía" in texto(x) for x in hijos(aviso)), "no existía: avisa antes de sustituirlo")
if aviso:
    responder(aviso, "No")
esperar(lambda: aviso and not u32.IsWindow(aviso))
ok(cerrar_sin_guardar(pi, v), "no existía: con «No» no guarda")
with open(ruta, encoding="utf-8") as f:
    ok(f.read() == "el de verdad", "no existía: el que apareció sigue igual")

# Ctrl+S escribe aparte: la ventana no espera al disco y lo que se escribe mientras queda por guardar.
ruta = os.path.join(dir, "de-fondo.txt")
with open(ruta, "w", encoding="utf-8", newline="") as f:
    f.write("hola\r\n")


def leer_fondo():
    try:
        with open(ruta, encoding="utf-8", newline="") as f:
            return f.read()
    except (PermissionError, FileNotFoundError):   # en el instante del reemplazo (si esta lectura lo estorba, Bloc
        return None                                 # pone el nuevo un momento después)


pi, v, e = abrir(ruta)
teclear(e, "x")
t0 = time.perf_counter()
orden(v, "Archivo", "Guardar")
ms = (time.perf_counter() - t0) * 1000
teclear(e, "y")
ok(esperar(lambda: leer_fondo() == "xhola\r\n") is not None and texto(v).startswith("*"),
   "de fondo: Ctrl+S devuelve la ventana en %.0f ms y guarda lo de antes; lo de después queda pendiente" % ms)
orden(v, "Archivo", "Guardar")
orden(v, "Archivo", "Guardar")
ok(esperar(lambda: not texto(v).startswith("*")) is not None and leer_fondo() == "xyhola\r\n",
   "de fondo: dos Ctrl+S seguidos dejan todo guardado")
teclear(e, "z")
orden(v, "Archivo", "Guardar", esperar_fin=False)
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi) and leer_fondo() == "xyzhola\r\n", "de fondo: cerrar justo tras Ctrl+S espera a que acabe y no pregunta")

# Un pegado que pasa de 1.000.000 de caracteres se deshace aunque al quitar el ajuste se rehaga la caja.
ruta = os.path.join(dir, "pegado.txt")
with open(ruta, "w", encoding="utf-8", newline="") as f:
    f.write(original)
pi, v, e = abrir(ruta)
u32.SendMessageW(e, EM_SETSEL, 0, -1)
enorme = ctypes.create_unicode_buffer("relleno " * 140000)
u32.SendMessageW(e, 0xC2, 1, ctypes.addressof(enorme))   # EM_REPLACESEL, como un pegado
time.sleep(0.5)
e = caja(v)
ok(u32.SendMessageW(e, WM_GETTEXTLENGTH, 0, 0) == 8 * 140000, "pegado enorme: entra entero")
orden(v, "Edición", "Deshacer")
e = caja(v)
ok(texto(e) == original, "pegado enorme: «Deshacer» devuelve el texto de antes")
u32.SendMessageW(e, EM_SETSEL, 0, 0)
teclear(e, "z ")
orden(v, "Edición", "Deshacer")
ok(texto(e) == "z" + original, "pegado enorme: luego deshace por palabras")
ok(cerrar_sin_guardar(pi, v), "pegado enorme: se cierra bien")

ruta = os.path.join(dir, "bloqueado.txt")
with open(ruta, "w", encoding="utf-8") as f:
    f.write("importante")
k32.CreateFileW.restype = w.HANDLE
cerrojo = k32.CreateFileW(ruta, 0x80000000, 0, None, 3, 0, None)   # abierto sin compartir, como hace Excel
pi, _ = lanzar(exe, ruta)
d = dialogo(pi.dwProcessId)
ok(d is not None, "bloqueado: avisa de que no lo puede abrir")
if d:
    responder(d, "Aceptar")
v = principal(pi.dwProcessId, "Sin título")
ok(v is not None, "bloqueado: queda un documento sin título, no ligado al archivo")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "bloqueado: se cierra bien")
k32.CloseHandle(cerrojo)
with open(ruta, encoding="utf-8") as f:
    ok(f.read() == "importante", "bloqueado: el archivo no se ha tocado")

# Tras uno de más de 1.000.000 de caracteres, «Nuevo» deja un documento normal y no ligado al grande.
antes = os.path.getmtime(grande)
pi, v, e = abrir(grande)
orden(v, "Ver", "Ajuste de línea")
ok(u32.IsWindow(e), "grande: no deja poner el ajuste de línea")
t0 = time.perf_counter()
orden(v, "Archivo", "Nuevo", esperar_fin=False)
ok(esperar(lambda: texto(v) == "Sin título - Bloc") is not None,
   "grande y luego «Nuevo»: queda sin título (%.0f ms)" % ((time.perf_counter() - t0) * 1000))
teclear(e, "abc ")
orden(v, "Edición", "Deshacer")
ok(texto(e) == "abc", "grande y luego «Nuevo»: vuelve a deshacer por palabras")
for _ in range(3):
    u32.SendMessageW(e, WM_KEYDOWN, 0x08, 0)   # RichEdit borra con la tecla, no con el carácter
ok(texto(e) == "" and texto(v) == "Sin título - Bloc", "nuevo vaciado: sin asterisco en el título")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "nuevo vaciado: se cierra sin preguntar si guardar")
ok(os.path.getmtime(grande) == antes, "grande y luego «Nuevo»: el grande no se ha tocado")

# «Nuevos documentos en Markdown»: el documento sin archivo pasa a Markdown con lo escrito y los nuevos empiezan así.
def en_markdown(v):
    """Se ve la barra de botones de formato: solo la tienen los Markdown."""
    return any(clase(x) == "BlocBotones" and u32.IsWindowVisible(x) for x in hijos(v))


pi, v, e = abrir(utf)
orden(v, "Archivo", "Nuevo")
teclear(e, "# Hola")
orden(v, "Ver", "Nuevos documentos en Markdown")
ok(en_markdown(v) and texto(e) == "# Hola" and texto(v) == "*Sin título - Bloc", "nuevos en Markdown: el sin título pasa a Markdown con lo escrito")
orden(v, "Archivo", "Nuevo", esperar_fin=False)
d = dialogo(pi.dwProcessId)
if d:
    u32.PostMessageW(d, WM_COMMAND, IDNO, 0)
ok(esperar(lambda: texto(v) == "Sin título - Bloc") is not None and en_markdown(v), "nuevos en Markdown: «Nuevo» empieza en Markdown")
orden(v, "Ver", "Nuevos documentos en Markdown")
ok(not en_markdown(v), "nuevos en Markdown: al quitarlo, vuelve a texto")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "nuevos en Markdown: se cierra bien")

pi, v, e = abrir(utf)
orden(v, "Ver", "Letra", esperar_fin=False)
d = dialogo(pi.dwProcessId)
if d:
    u32.PostMessageW(d, WM_COMMAND, IDOK, 0)
    esperar(lambda: not u32.IsWindow(d))
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(d is not None and terminar(pi), "letra: el diálogo sale y se acepta")
with open(INI, encoding="utf-8") as f:
    ok("tamano=11" in f.read(), "letra: aceptar sin tocar nada deja el tamaño como estaba")

# --- Modo noche: se mide el brillo de cada parte de la ventana, pintada como la pinta Windows ---
g32 = ctypes.windll.gdi32
for f, res, args in ((u32.GetWindowDC, w.HDC, [w.HWND]), (g32.CreateCompatibleDC, w.HDC, [w.HDC]),
                     (g32.CreateCompatibleBitmap, w.HBITMAP, [w.HDC, ctypes.c_int, ctypes.c_int]),
                     (g32.SelectObject, w.HGDIOBJ, [w.HDC, w.HGDIOBJ]), (g32.GetPixel, w.COLORREF, [w.HDC, ctypes.c_int, ctypes.c_int]),
                     (u32.PrintWindow, w.BOOL, [w.HWND, w.HDC, w.UINT]), (u32.ReleaseDC, ctypes.c_int, [w.HWND, w.HDC]),
                     (g32.DeleteDC, w.BOOL, [w.HDC]), (g32.DeleteObject, w.BOOL, [w.HGDIOBJ]), (u32.GetDC, w.HDC, [w.HWND]),
                     (g32.BitBlt, w.BOOL, [w.HDC] + [ctypes.c_int] * 4 + [w.HDC, ctypes.c_int, ctypes.c_int, w.DWORD]),
                     (g32.GetDIBits, ctypes.c_int, [w.HDC, w.HBITMAP, w.UINT, w.UINT, ctypes.c_void_p, ctypes.c_void_p, w.UINT])):
    f.restype, f.argtypes = res, args


def rect(h):
    r = w.RECT(); u32.GetWindowRect(h, ctypes.byref(r)); return r


def brillos(v):
    """Brillo medio (0 a 255) de la barra de menús, la de botones, el texto, su barra de desplazamiento y la de estado."""
    rv, re = rect(v), rect(caja(v))
    hs = {clase(x): rect(x) for x in hijos(v)}
    est, bot = hs["BlocEstado"], hs["BlocBotones"]
    o = w.POINT(); u32.ClientToScreen(v, ctypes.byref(o))
    puntos = {"menús": (rv.right - 40, o.y - 5), "texto": (re.left + 10, re.bottom - 10),
              "desplazamiento": (re.right - 6, re.top + 60), "botones": (bot.right - 10, (bot.top + bot.bottom) // 2),
              "estado": (est.left + (est.right - est.left) * 2 // 5, (est.top + est.bottom) // 2)}
    dc = u32.GetWindowDC(v); m = g32.CreateCompatibleDC(dc)
    b = g32.CreateCompatibleBitmap(dc, rv.right - rv.left, rv.bottom - rv.top)
    g32.SelectObject(m, b); u32.PrintWindow(v, m, 2)
    res = {}
    for k, (x, y) in puntos.items():
        c = g32.GetPixel(m, x - rv.left, y - rv.top)
        res[k] = ((c & 255) + (c >> 8 & 255) + (c >> 16 & 255)) // 3
    g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(v, dc)
    return res


ruta = os.path.join(dir, "noche.md")
with open(ruta, "w", encoding="utf-8") as f:
    f.write("# Noche\n\nTexto.\n")
pi, v, e = abrir(ruta)
time.sleep(0.5)
b = brillos(v)
ok(min(b.values()) > 190, "noche: empieza claro, como dice el ini %s" % b)
orden(v, "Ver", "Modo noche")
time.sleep(1)
b = brillos(v)
ok(max(b.values()) < 80, "noche: se oscurece todo %s" % b)
orden(v, "Ver", "Ajuste de línea")
orden(v, "Ver", "Ajuste de línea")
b = brillos(v)
ok(b["texto"] < 80 and b["desplazamiento"] < 80, "noche: sigue oscuro al cambiar el ajuste de línea")
orden(v, "Ver", "Modo noche")
time.sleep(1)
b = brillos(v)
ok(min(b.values()) > 190, "noche: vuelve a claro %s" % b)
orden(v, "Ver", "Modo noche")
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "noche: se cierra bien")
with open(INI, encoding="utf-8") as f:
    ok("noche=1" in f.read(), "noche: se guarda en el ini")
pi, v, e = abrir(ruta)
time.sleep(0.5)
b = brillos(v)
ok(max(b.values()) < 80, "noche: al volver a abrir, empieza oscuro %s" % b)
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "noche: se cierra bien otra vez")
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=150\nnoche=0\n")


def zoom_de(pi, e):
    """EM_GETZOOM, con la memoria para su respuesta dentro de Bloc."""
    k32.VirtualAllocEx.restype = ctypes.c_void_p
    k32.VirtualAllocEx.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_size_t, w.DWORD, w.DWORD]
    k32.ReadProcessMemory.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
    k32.VirtualFreeEx.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_size_t, w.DWORD]
    m = k32.VirtualAllocEx(pi.hProcess, None, 8, 0x3000, 4)
    u32.SendMessageW(e, 0x4E0, m, m + 4)
    r = (ctypes.c_int32 * 2)()
    k32.ReadProcessMemory(pi.hProcess, m, r, 8, None)
    k32.VirtualFreeEx(pi.hProcess, m, 0, 0x8000)
    return r[0] * 100 // r[1] if r[0] and r[1] else 100


def pixeles(e):
    """Lo que se ve del texto: ancho, alto y cada píxel en 0xRRGGBB."""
    r = rect(e); an, al = r.right - r.left, r.bottom - r.top
    dc = u32.GetDC(e); m = g32.CreateCompatibleDC(dc); b = g32.CreateCompatibleBitmap(dc, an, al)
    antes = g32.SelectObject(m, b); u32.PrintWindow(e, m, 2)   # el escritorio oculto no se deja copiar con BitBlt desde aquí
    g32.SelectObject(m, antes)   # GetDIBits no lee un mapa de bits que esté puesto en un DC
    info = (ctypes.c_int32 * 10)(40, an, -al, 1 | (32 << 16), 0, 0, 0, 0, 0, 0)
    px = (ctypes.c_uint32 * (an * al))()
    g32.GetDIBits(m, b, 0, al, px, info, 0)
    g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(e, dc)
    return an, al, px


def posicion(pi, e, c):
    """EM_POSFROMCHAR del carácter c, con la memoria para su respuesta dentro de Bloc."""
    m = k32.VirtualAllocEx(pi.hProcess, None, 8, 0x3000, 4)
    u32.SendMessageW(e, 0xD6, m, c)
    r = (ctypes.c_int32 * 2)()
    k32.ReadProcessMemory(pi.hProcess, m, r, 8, None)
    k32.VirtualFreeEx(pi.hProcess, m, 0, 0x8000)
    return r[0], r[1]


def raya_mas_larga(e):
    """La tira horizontal más larga del color de la raya en claro (234, 236, 239; ver tema.rs) en lo que se ve del texto,
    en píxeles."""
    an, al, px = pixeles(e)
    mejor = 0
    for y in range(al):
        seguidos = 0
        for x in range(an):
            seguidos = seguidos + 1 if px[y * an + x] & 0xFFFFFF == 0xEAECEF else 0
            mejor = max(mejor, seguidos)
    return mejor


pi, v, e = abrir(ruta)
time.sleep(0.3)
ok(zoom_de(pi, e) == 150, "el zoom guardado se pone al abrir (%d %%)" % zoom_de(pi, e))
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "zoom: se cierra bien")
raya = os.path.join(dir, "raya.md")
with open(raya, "w", encoding="utf-8") as f:
    f.write("Antes\n\n---\n\nDespués\n")
pi, v, e = abrir(raya)
time.sleep(0.5)
ok(raya_mas_larga(e) >= 300, "la raya --- se ve como una línea (%d píxeles con zoom 150)" % raya_mas_larga(e))
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "zoom: se cierra bien")

# Con zoom, RichEdit agranda o encoge los márgenes: lo que Bloc pinta en el margen no puede tapar la primera letra (al
# 80 % tapaba la primera «M» de cada línea), y la primera línea no toca la barra.
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=80\nnoche=0\n")
emes = os.path.join(dir, "margen.md")
with open(emes, "w", encoding="utf-8") as f:
    f.write("MMMMMMMMMMMMMMMMMMMM\n")
pi, v, e = abrir(emes)
time.sleep(0.5)
x, y = posicion(pi, e, 0)
an, al, px = pixeles(e)
tinta = [c for c in range(an) for f in range(y, min(y + 12, al)) if sum((px[f * an + c] >> s) & 0xFF for s in (0, 8, 16)) < 400]
ok(tinta and min(tinta) <= x + 2, "al 80 %%, la primera letra empieza donde la pone RichEdit (en %s; tinta desde %s)" % (x, min(tinta) if tinta else None))
ok(y >= 4, "la primera línea deja un hueco bajo la barra (%d píxeles al 80 %%)" % y)
u32.PostMessageW(v, WM_CLOSE, 0, 0)
ok(terminar(pi), "márgenes: se cierra bien")
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")

# Abierto con doble clic, la carpeta de trabajo es la del archivo: Bloc no puede quedársela, o Windows no deja cambiarle
# el nombre mientras esté abierto.
carpeta = os.path.join(dir, "carpeta-cliente")
os.makedirs(carpeta, exist_ok=True)
with open(os.path.join(carpeta, "nota.txt"), "w", encoding="utf-8") as f:
    f.write("nota")
si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO; pi = PI()
k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" nota.txt' % exe), None, None, False, 0, None, carpeta,
                   ctypes.byref(si), ctypes.byref(pi))
u32.WaitForInputIdle(pi.hProcess, 15000)
v = principal(pi.dwProcessId, "nota.txt")
e = caja(v) if v else None
ok(e is not None and texto(e) == "nota", "abre un archivo dado con su ruta relativa a la carpeta de trabajo")
try:
    os.rename(carpeta, carpeta + "-2")
    os.rename(carpeta + "-2", carpeta)
    se_puede = True
except OSError:
    se_puede = False
ok(se_puede, "con Bloc abierto, la carpeta del archivo se puede renombrar")
ok(cerrar_sin_guardar(pi, v), "carpeta: se cierra bien")

# msftedit.dll se carga de Windows, no de junto al .exe (una copia puesta en la carpeta de red).
junto = os.path.join(dir, "con-dll")
os.makedirs(junto, exist_ok=True)
copia = os.path.join(junto, "Bloc.exe")
with open(exe, "rb") as f, open(copia, "wb") as g:
    g.write(f.read())
with open(os.path.join(junto, "msftedit.dll"), "wb") as f:
    f.write(b"MZ no es una biblioteca")
with open(os.path.join(junto, "dll.txt"), "w", encoding="utf-8") as f:
    f.write("con la de Windows")
pi, _ = lanzar(copia, os.path.join(junto, "dll.txt"))
v = principal(pi.dwProcessId, "dll.txt")
e = caja(v) if v else None
ok(e is not None and texto(e) == "con la de Windows", "con un msftedit.dll falso junto al .exe, usa el de Windows")
ok(v is not None and cerrar_sin_guardar(pi, v), "dll: se cierra bien")
if v is None:
    k32.TerminateProcess(pi.hProcess, 0)

# --- Archivo › Recientes: la lista va en el bloc.ini y la ven todas las ventanas ---
WM_INITMENUPOPUP = 0x117
u32.GetMenuState.argtypes = [w.HMENU, w.UINT, w.UINT]


def recientes_ini():
    with open(INI, encoding="utf-8") as f:
        return [x[len("reciente="):] for x in f.read().splitlines() if x.startswith("reciente=")]


def igual(a, b):
    return os.path.normcase(os.path.abspath(a)) == os.path.normcase(os.path.abspath(b))


def menu_recientes(v):
    """Lo que enseña Archivo › Recientes al abrir Archivo, lo que tarda en llenarse y si sale en gris."""
    archivo = u32.GetSubMenu(u32.GetMenu(v), 0)
    t0 = time.perf_counter()
    u32.SendMessageW(v, WM_INITMENUPOPUP, archivo, 0)
    ms = (time.perf_counter() - t0) * 1000
    m = u32.GetSubMenu(archivo, 3)
    nombres = []
    for i in range(u32.GetMenuItemCount(m)):
        b = ctypes.create_unicode_buffer(512)
        u32.GetMenuStringW(m, i, b, 512, 0x400)   # MF_BYPOSITION
        nombres.append(b.value)
    return nombres, ms, bool(u32.GetMenuState(archivo, 3, 0x400) & 0x1)   # MF_GRAYED


ra, rb, rd = (os.path.join(dir, n) for n in ("reciente-a.md", "reciente-b.txt", "reciente-d.md"))
for r in (ra, rb):
    with open(r, "w", encoding="utf-8") as f:
        f.write("x")
pi1, v1, e1 = abrir(ra)
ok(igual(recientes_ini()[0], ra), "recientes: abrir pone el archivo arriba")
pi2, v2, e2 = abrir(rb)
nombres, ms, _ = menu_recientes(v1)
ok(nombres[0].startswith("&1 reciente-b.txt\t") and nombres[1].startswith("&2 reciente-a.md\t") and nombres[-1] == "&Borrar la lista",
   "recientes: lo abierto en otra ventana sale arriba en esta (%s)" % nombres[:2])
orden(v2, "Archivo", "Guardar como", esperar_fin=False)
d = dialogo(pi2.dwProcessId, "Guardar como")
if d:
    nombre_archivo = next(x for x in hijos(d) if clase(x) == "Edit" and clase(u32.GetParent(x)) == "ComboBox")
    b = ctypes.create_unicode_buffer(os.path.normpath(rd))   # en una variable: si no, se libera antes de usarlo
    u32.SendMessageW(nombre_archivo, WM_SETTEXT, 0, ctypes.addressof(b))
    u32.PostMessageW(next(x for x in hijos(d) if clase(x) == "Button" and u32.GetDlgCtrlID(x) == IDOK), 0xF5, 0, 0)   # BM_CLICK en Guardar
ok(esperar(lambda: igual(recientes_ini()[0], rd), 5) is not None, "recientes: «Guardar como» pone el nuevo arriba")
os.remove(ra)
u32.PostMessageW(v2, WM_COMMAND, 1090 + next(i for i, r in enumerate(recientes_ini()) if igual(r, ra)), 0)
d = dialogo(pi2.dwProcessId)
ok(d is not None and any("No se encuentra" in texto(x) for x in hijos(d)), "recientes: uno borrado lo dice al pulsarlo")
if d:
    responder(d, "Aceptar")
ok(esperar(lambda: not any(igual(r, ra) for r in recientes_ini()), 3) is not None, "recientes: y lo quita de la lista")
with open(INI, encoding="utf-8") as f:
    resto = [x for x in f.read().splitlines() if not x.startswith("reciente=")]
red = [r"\\bloc-no-existe-%d\compartida\carpeta\nota-%d.md" % (i, i) for i in range(8)]
with open(INI, "w", encoding="utf-8") as f:
    f.write("\n".join(resto + ["reciente=" + r for r in red]) + "\n")
nombres, ms, gris = menu_recientes(v1)
ok(len(nombres) == 10 and nombres[0].startswith("&1 nota-0.md\t") and not gris and ms < 50,
   "recientes: con 8 rutas de red apagadas, el menú se llena en %.1f ms sin mirarlas" % ms)
u32.SendMessageW(v1, WM_COMMAND, 1098, 0)
nombres, ms, gris = menu_recientes(v1)
ok(recientes_ini() == [] and gris, "recientes: «Borrar la lista» la vacía y Recientes sale en gris")
ok(cerrar_sin_guardar(pi1, v1) and cerrar_sin_guardar(pi2, v2), "recientes: las dos ventanas se cierran")
ok(recientes_ini() == [], "recientes: al cerrarse, ninguna ventana vuelve a poner la lista que tenía")
for r in (rb, rd):
    os.remove(r)

# --- Cerrar: «Seguir luego» y la nota para la próxima vez (en la carpeta del bloc.ini de la prueba) ---
NOTA = os.path.join(dir, "nota.bloc")
SEGUIR, ANADIR, SUSTITUIR = 10, 11, 12
WM_QUERYENDSESSION = 0x11
BARRA = "\r\n" * 6 + "=" * 40 + "\r\n\r\n"


def sin_archivo():
    """Bloc abierto como desde Inicio, sin archivo."""
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO
    pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s"' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    u32.WaitForInputIdle(pi.hProcess, 15000)
    v = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "BlocVentana"), None))
    return pi, v, caja(v) if v else None


def cerrar_con(pi, v, boton):
    """Cierra con cambios y pulsa ese botón del aviso; devuelve lo que tardó en salir el aviso, en ms."""
    t0 = time.perf_counter()
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    d = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"), None), 5)
    ms = (time.perf_counter() - t0) * 1000
    if d:
        u32.PostMessageW(d, WM_COMMAND, boton, 0)
    return d, ms


def nota_texto():
    """El texto de la nota guardada, o None."""
    if not os.path.exists(NOTA):
        return None
    with open(NOTA, "rb") as f:
        b = f.read()
    return b[b.index(b"\n\n") + 2:].decode("utf-16-le")


def guardar_sin(v):
    u32.SendMessageW(v, WM_COMMAND, 1003, 0)   # Archivo › Guardar


def bytes_de(r):
    """Lo que tiene el archivo, o None si aún no está o Bloc lo está escribiendo."""
    try:
        with open(r, "rb") as f:
            return f.read()
    except OSError:
        return None


# Seguir, abrir y cursor; con su archivo, Ctrl+S guarda en él y la nota se va.
ra = os.path.join(dir, "seguir-a.md")
with open(ra, "w", encoding="utf-8", newline="") as f:
    f.write("# Uno\r\n\r\ntexto del archivo\r\n")
pi, v, e = abrir(ra)
teclear(e, "nuevo ")
u32.SendMessageW(e, EM_SETSEL, 4, 4)
d, ms = cerrar_con(pi, v, SEGUIR)
ok(d is not None and ms < 100 and terminar(pi), "seguir: el aviso sale en %.0f ms y «Seguir luego» cierra" % ms)
ok(nota_texto() == "nuevo # Uno\r\rtexto del archivo\r", "seguir: la nota guarda el texto tal cual (%r)" % nota_texto())
with open(ra, encoding="utf-8", newline="") as f:
    ok(f.read() == "# Uno\r\n\r\ntexto del archivo\r\n", "seguir: el archivo no se toca")
pi, v, e = sin_archivo()
ok(v is not None and texto(v).startswith("*seguir-a.md") and texto(e) == "nuevo # Uno\r\n\r\ntexto del archivo\r\n",
   "seguir: Bloc sin archivo abre la nota, marcada como cambiada y con su archivo (%r)" % (texto(v) if v else None))
ok(seleccion(e) == (4, 4), "seguir: con el cursor donde estaba")
pi2, v2, e2 = sin_archivo()
ok(v2 is not None and texto(e2) == "", "seguir: otra ventana que se abre a la vez sale vacía")
terminar(pi2, 0)
guardar_sin(v)
ok(esperar(lambda: bytes_de(ra) == b"nuevo # Uno\r\n\r\ntexto del archivo\r\n", 3) is not None,
   "seguir: Ctrl+S guarda la nota en su archivo")
ok(esperar(lambda: not os.path.exists(NOTA), 3) is not None, "seguir: y la nota se borra")
ok(cerrar_sin_guardar(pi, v), "seguir: después se cierra sin preguntar")

# Matar el proceso con la nota abierta: sigue ahí. No guardar la borra.
pi, v, e = abrir(ra)
teclear(e, "z")
cerrar_con(pi, v, SEGUIR)
terminar(pi)
pi, v, e = sin_archivo()
k32.TerminateProcess(pi.hProcess, 1)
k32.WaitForSingleObject(pi.hProcess, 5000)
ok(nota_texto() is not None, "seguir: si Bloc acaba de golpe con la nota abierta, la nota sigue")
pi, v, e = sin_archivo()
ok(v is not None and texto(e).startswith("z"), "seguir: y se vuelve a abrir")
d, _ = cerrar_con(pi, v, 7)   # No guardar
ok(d is not None and terminar(pi) and not os.path.exists(NOTA), "seguir: «No guardar» la borra")

# Su archivo, cambiado o borrado entretanto.
pi, v, e = abrir(ra)
teclear(e, "y")
cerrar_con(pi, v, SEGUIR)
terminar(pi)
time.sleep(0.05)
with open(ra, "w", encoding="utf-8", newline="") as f:
    f.write("lo de otro")
pi, v, e = sin_archivo()
u32.PostMessageW(v, WM_COMMAND, 1003, 0)
d = dialogo(pi.dwProcessId)
ok(d is not None and any("ha cambiado desde que lo dejaste" in texto(x) for x in hijos(d)), "seguir: si su archivo ha cambiado entretanto, lo pregunta")
if d:
    u32.PostMessageW(d, WM_COMMAND, SUSTITUIR, 0)
ok(esperar(lambda: (bytes_de(ra) or b"").startswith(b"ynuevo"), 3) is not None and esperar(lambda: not os.path.exists(NOTA), 3) is not None,
   "seguir: «Sustituirlo» lo guarda encima y la nota se va")
cerrar_sin_guardar(pi, v)
pi, v, e = abrir(ra)
teclear(e, "w")
cerrar_con(pi, v, SEGUIR)
terminar(pi)
os.remove(ra)
pi, v, e = sin_archivo()
u32.PostMessageW(v, WM_COMMAND, 1003, 0)
d = dialogo(pi.dwProcessId, "Guardar como")
ok(d is not None, "seguir: si su archivo ya no está, Guardar abre «Guardar como»")
rb = os.path.join(dir, "seguir-b.md")
if d:
    nombre_archivo = next(x for x in hijos(d) if clase(x) == "Edit" and clase(u32.GetParent(x)) == "ComboBox")
    poner(nombre_archivo, os.path.normpath(rb))
    u32.PostMessageW(next(x for x in hijos(d) if clase(x) == "Button" and u32.GetDlgCtrlID(x) == IDOK), 0xF5, 0, 0)
ok(esperar(lambda: os.path.exists(rb), 5) is not None and esperar(lambda: not os.path.exists(NOTA), 3) is not None, "seguir: y la nota se va")
cerrar_sin_guardar(pi, v)

# Ya hay una nota: Volver no cierra; Añadir la deja al final con la barra, «Sin título» y con los bytes exactos.
pa, va, ea = abrir(os.path.join(dir, "no-existe-a.txt"))
teclear(ea, "uno\r\r")
cerrar_con(pa, va, SEGUIR)
terminar(pa)
pb, vb, eb = abrir(os.path.join(dir, "no-existe-b.md"))
teclear(eb, "dos")
def ya_hay(pid):
    """El aviso de «Ya hay una nota» (el de cerrar puede no haberse ido aún)."""
    return esperar(lambda: next((h for h in ventanas(pid) if clase(h) == "#32770" and any("Ya hay una nota" in texto(x) for x in hijos(h))), None), 5)


cerrar_con(pb, vb, SEGUIR)
d = ya_hay(pb.dwProcessId)
ok(d is not None and any("uno" in texto(x) for x in hijos(d)), "seguir: si ya hay una nota, lo dice con su primera línea")
if d:
    u32.PostMessageW(d, WM_COMMAND, 2, 0)   # Volver (Esc)
ok(esperar(lambda: not u32.IsWindowVisible(d), 3) is not None and u32.IsWindowVisible(vb) and nota_texto() == "uno\r\r", "seguir: «Volver» no cierra ni toca la nota")
cerrar_con(pb, vb, SEGUIR)
d = ya_hay(pb.dwProcessId)
if d:
    u32.PostMessageW(d, WM_COMMAND, ANADIR, 0)
ok(terminar(pb) and nota_texto() == "uno" + BARRA.replace("\r\n", "\r") + "dos", "seguir: «Añadir» la deja al final, con 5 líneas vacías y la barra (%r)" % nota_texto())
pi, v, e = sin_archivo()
ok(v is not None and texto(v).startswith("*Sin título"), "seguir: la nota unida es «Sin título»")
u32.PostMessageW(v, WM_COMMAND, 1003, 0)
d = dialogo(pi.dwProcessId, "Guardar como")
rc = os.path.join(dir, "seguir-c.txt")
if d:
    nombre_archivo = next(x for x in hijos(d) if clase(x) == "Edit" and clase(u32.GetParent(x)) == "ComboBox")
    poner(nombre_archivo, os.path.normpath(rc))
    u32.PostMessageW(next(x for x in hijos(d) if clase(x) == "Button" and u32.GetDlgCtrlID(x) == IDOK), 0xF5, 0, 0)
ok(esperar(lambda: bytes_de(rc) == ("uno" + BARRA + "dos").encode("utf-8"), 5) is not None, "seguir: guardada, los bytes exactos del separador")
cerrar_sin_guardar(pi, v)

# Apagado: sin preguntar, a la nota; dos ventanas que cierran a la vez no se pisan.
pi, v, e = abrir(os.path.join(dir, "no-existe-c.txt"))
teclear(e, "al apagar")
r = u32.SendMessageW(v, WM_QUERYENDSESSION, 0, 0)
ok(r == 1 and nota_texto() == "al apagar" and not dialogo(pi.dwProcessId), "seguir: al apagar, sin avisos, a la nota")
terminar(pi, 0)
# Con la nota esperando, abierto con un archivo: está arriba en Recientes y se abre desde ahí.
pi, v, e = abrir(rb)
nombres, _, _ = menu_recientes(v)
ok(nombres[:1] == ["Nota de la última vez"], "seguir: con un archivo, la nota espera arriba en Recientes (%s)" % nombres[:2])
u32.SendMessageW(v, WM_COMMAND, 1099, 0)
ok(texto(e) == "al apagar" and texto(v).startswith("*"), "seguir: y se abre desde ahí")
cerrar_con(pi, v, 7)
terminar(pi)
# Tras reiniciar Windows por una actualización, Bloc solo vuelve si hay nota.
pi, _ = lanzar(exe, "/reinicio")
ok(terminar(pi, 3) and not ventanas(pi.dwProcessId), "seguir: tras reiniciar sin nota, Bloc no abre nada")
pi, v, e = abrir(os.path.join(dir, "no-existe-g.txt"))
teclear(e, "tras reiniciar")
cerrar_con(pi, v, SEGUIR)
terminar(pi)
pi, _ = lanzar(exe, "/reinicio")
v = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "BlocVentana"), None))
ok(v is not None and texto(caja(v)) == "tras reiniciar", "seguir: tras reiniciar con nota, Bloc vuelve con ella")
if v:
    cerrar_con(pi, v, 7)
terminar(pi)
p1, v1, e1 = abrir(os.path.join(dir, "no-existe-d.txt"))
p2, v2, e2 = abrir(os.path.join(dir, "no-existe-e.txt"))
teclear(e1, "primera")
teclear(e2, "segunda")
for v in (v1, v2):
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
avisos = [dialogo(p.dwProcessId) for p in (p1, p2)]
for d in avisos:
    if d:
        u32.PostMessageW(d, WM_COMMAND, SEGUIR, 0)
time.sleep(0.5)
for p in (p1, p2):
    d = next((h for h in ventanas(p.dwProcessId) if clase(h) == "#32770"), None)
    if d:
        u32.PostMessageW(d, WM_COMMAND, ANADIR, 0)
ok(terminar(p1) and terminar(p2) and nota_texto() in ("primera" + BARRA.replace("\r\n", "\r") + "segunda", "segunda" + BARRA.replace("\r\n", "\r") + "primera"),
   "seguir: dos ventanas que cierran a la vez, las dos en la nota (%r)" % nota_texto())
os.remove(NOTA)

# 900 KB y una carpeta de red apagada: el aviso al momento y nada se pierde.
grande_md = os.path.join(dir, "seguir-900.md")
with open(grande_md, "w", encoding="utf-8", newline="") as f:
    f.write("".join("# Título %d\r\n\r\nPárrafo %d con algo de texto de relleno para llenar la línea entera.\r\n\r\n- uno\r\n- dos\r\n\r\n" % (i, i) for i in range(9000)))
with open(grande_md, encoding="utf-8", newline="") as f:
    original900 = f.read()
pi, v, e = abrir(grande_md)
teclear(e, "Q")
time.sleep(0.5)
d, ms = cerrar_con(pi, v, SEGUIR)
ok(d is not None and ms < 100 and terminar(pi, 10), "seguir: en 900 KB, el aviso en %.0f ms" % ms)
pi, v, e = sin_archivo()
ok(v is not None and texto(e) == "Q" + original900, "seguir: en 900 KB, la nota vuelve igual")
cerrar_con(pi, v, 7)
terminar(pi)
red = r"\\bloc-no-existe-k10\compartida\nota.md"
pi, v, e = abrir(red)
teclear(e, "en la red")
t0 = time.perf_counter()
d, ms = cerrar_con(pi, v, SEGUIR)
ok(d is not None and ms < 100 and terminar(pi, 3), "seguir: con su carpeta de red apagada, el aviso en %.0f ms y cierra en %.0f ms" % (ms, (time.perf_counter() - t0) * 1000))
ok(nota_texto() == "en la red", "seguir: y la nota lo tiene")
os.remove(NOTA)

# --- Copia mientras escribes: 2 s después de la última tecla; al abrir tras un cierre de golpe, pasa a la nota ---
class IO(ctypes.Structure):
    _fields_ = [(n, ctypes.c_ulonglong) for n in ("lecturas", "escrituras", "otras", "leido", "escrito", "otro")]


def escrituras(pi):
    io = IO()
    k32.GetProcessIoCounters(pi.hProcess, ctypes.byref(io))
    return io.escrituras


def copia_de(pi):
    return os.path.join(dir, "copia-%d.bloc" % pi.dwProcessId)


def matar(pi):
    k32.TerminateProcess(pi.hProcess, 1)
    k32.WaitForSingleObject(pi.hProcess, 5000)


rk = os.path.join(dir, "copia-a.md")
with open(rk, "w", encoding="utf-8", newline="") as f:
    f.write("base\r\n")
pi, v, e = abrir(rk)
teclear(e, "uno ")
ok(esperar(lambda: os.path.exists(copia_de(pi)), 4) is not None, "copia: unos 2 s después de escribir, hay copia")
time.sleep(0.2)
teclear(e, "dos ")
time.sleep(0.5)   # menos de 2 s: lo último no llega a la copia
matar(pi)
pi, v, e = sin_archivo()
ok(v is not None and texto(e) == "uno base\r\n" and texto(v).startswith("*copia-a.md"),
   "copia: tras un cierre de golpe, al abrir Bloc está todo menos los últimos 2 s (%r)" % (texto(e) if e else None))
ok(not [n for n in os.listdir(dir) if n.startswith("copia-") and n.endswith(".bloc")], "copia: y la copia ha pasado a la nota")
# Desde la nota: lo escrito encima la sustituye al recuperarla, sin repetirla.
u32.SendMessageW(e, EM_SETSEL, 0, 0)
teclear(e, "tres ")
copia = copia_de(pi)
ok(esperar(lambda: os.path.exists(copia), 4) is not None, "copia: también de la nota abierta")
time.sleep(0.2)
matar(pi)
pi, v, e = sin_archivo()
ok(v is not None and texto(e) == "tres uno base\r\n", "copia: la de la nota la sustituye, sin repetirla (%r)" % (texto(e) if e else None))
guardar_sin(v)
ok(esperar(lambda: bytes_de(rk) == b"tres uno base\r\n", 3) is not None and esperar(lambda: not os.path.exists(NOTA), 3) is not None,
   "copia: guardada en su archivo, la nota se va")
u32.SendMessageW(e, EM_SETSEL, 0, 0)
teclear(e, "x")
ok(esperar(lambda: os.path.exists(copia_de(pi)), 4) is not None, "copia: escribir otra vez hace copia")
guardar_sin(v)
ok(esperar(lambda: not os.path.exists(copia_de(pi)), 3) is not None, "copia: al guardar, la copia se va")
u32.SendMessageW(e, EM_SETSEL, 0, 0)
teclear(e, "y")
esperar(lambda: os.path.exists(copia_de(pi)), 4)
cerrar_con(pi, v, SEGUIR)
ok(terminar(pi) and not os.path.exists(copia_de(pi)) and nota_texto() == "yxtres uno base\r", "copia: con «Seguir luego», pasa a la nota")
os.remove(NOTA)
# Desmarcada: ni un byte al disco.
pi, v, e = abrir(rk)
orden(v, "Archivo", "Guardar copia mientras escribes")
antes = escrituras(pi)
teclear(e, "sin copia ")
time.sleep(3)
ok(escrituras(pi) == antes and not os.path.exists(copia_de(pi)), "copia: desmarcada, nada se escribe en el disco (%d escrituras)" % (escrituras(pi) - antes))
cerrar_con(pi, v, 7)
terminar(pi)
with open(INI, encoding="utf-8") as f:
    ok("copia=0" in f.read(), "copia: la opción se guarda en el bloc.ini")
with open(INI, encoding="utf-8") as f:
    ini = f.read().replace("copia=0", "copia=1")
with open(INI, "w", encoding="utf-8") as f:
    f.write(ini)

# --- Imprimir: en un escritorio aparte, el diálogo clásico; con BLOC_IMPRIMIR, el camino de la ventana de Windows sin
# ella (las páginas, la vista previa, el trabajo a «Microsoft Print to PDF» y las mismas páginas por GDI) ---
psapi = ctypes.windll.psapi
psapi.EnumProcessModulesEx.argtypes = [w.HANDLE, ctypes.POINTER(w.HMODULE), w.DWORD, ctypes.POINTER(w.DWORD), w.DWORD]
psapi.GetModuleBaseNameW.argtypes = [w.HANDLE, w.HMODULE, w.LPWSTR, w.DWORD]


def modulos(pi):
    hs = (w.HMODULE * 1024)(); n = w.DWORD()
    psapi.EnumProcessModulesEx(pi.hProcess, hs, ctypes.sizeof(hs), ctypes.byref(n), 3)
    res = set()
    for h in hs[:n.value // ctypes.sizeof(w.HMODULE)]:
        b = ctypes.create_unicode_buffer(260)
        psapi.GetModuleBaseNameW(pi.hProcess, h, b, 260)
        res.add(b.value.lower())
    return res


DIRECT2D = {"d2d1.dll", "d3d11.dll", "dxgi.dll", "dwrite.dll"}
ri = os.path.join(dir, "imprimir.md")
with open(ri, "w", encoding="utf-8", newline="") as f:
    f.write("# Imprimir\r\n\r\nUn párrafo con **negrita** y `código`.\r\n\r\n> Una cita.\r\n\r\n```\r\n"
            + "".join("linea_%02d = calcula(%d)\r\n" % (i, i) for i in range(40)) + "```\r\n\r\n| N | Concepto |\r\n|---|---|\r\n"
            + "".join("| %d | Concepto número %d |\r\n" % (i, i) for i in range(1, 70)) + "\r\nLínea final.\r\n")
pi, v, e = abrir(ri)
ok(not DIRECT2D & modulos(pi), "imprimir: al abrir no se carga nada de Direct2D")
orden(v, "Archivo", "Imprimir", esperar_fin=False)
d = dialogo(pi.dwProcessId)
ok(d is not None, "imprimir: fuera del escritorio de verdad, el diálogo clásico (la ventana de Windows saldría en el del usuario)")
if d:
    u32.PostMessageW(d, WM_COMMAND, 2, 0)   # Cancelar
    ok(esperar(lambda: not [h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"], 3), "imprimir: cancelar lo cierra")
# El diálogo de Windows carga por su cuenta d2d1.dll; Bloc, para la ventana moderna, también d3d11.dll y dwrite.dll.
cargadas = DIRECT2D & modulos(pi)
ok("d3d11.dll" not in cargadas and texto(e).startswith("# Imprimir"), "imprimir: Bloc no carga Direct2D ni cambia el texto (del diálogo: %s)" % sorted(cargadas))
cerrar_sin_guardar(pi, v)
salida = os.path.join(dir, "imprimir")
os.makedirs(salida, exist_ok=True)
for n in os.listdir(salida):
    os.remove(os.path.join(salida, n))
os.environ["BLOC_IMPRIMIR"] = salida
pi, v, e = abrir(ri)
del os.environ["BLOC_IMPRIMIR"]
t0 = time.perf_counter()
orden(v, "Archivo", "Imprimir", esperar_fin=False)
fin = os.path.join(salida, "fin.txt")
esperar(lambda: os.path.exists(fin), 60)
ms = (time.perf_counter() - t0) * 1000
informe = (bytes_de(fin) or b"").decode("utf-8")
cuantas = lambda que: int(next((l.split(":")[1].split()[0] for l in informe.splitlines() if l.startswith(que + ":")), "0"))
vertical, apaisado = cuantas("vertical"), cuantas("apaisado")
ok(vertical >= 3 and apaisado > vertical, "imprimir: %d páginas en vertical y %d en apaisado (%.0f ms todo)" % (vertical, apaisado, ms))
ok(all(x + ": bien" in informe for x in ("previa", "trabajo", "gdi")), "imprimir: vista previa, trabajo y PDF por GDI hechos (%r)" % informe)
ok(len([n for n in os.listdir(salida) if n.startswith("pagina-")]) == vertical, "imprimir: una página en metarchivo por hoja")
import re
for nombre in ("trabajo.oxps", "gdi.pdf"):
    b = bytes_de(os.path.join(salida, nombre)) or b""
    ok(b.startswith(b"%PDF") and not re.search(rb"/Subtype\s*/Image", b) and re.search(rb"/Type\s*/Font", b) and b"/ToUnicode" in b,
       "imprimir: %s es un PDF con letras y su texto, sin imágenes (%d KB)" % (nombre, len(b) // 1024))
try:
    import fitz
    pdf = fitz.open(os.path.join(salida, "trabajo.oxps"))
    letras = "".join(str(p.get_text()) for p in pdf)
    ok("Concepto número 69" in letras and "linea_39" in letras and "Una cita." in letras, "imprimir: el texto del PDF se puede copiar")
except ImportError:
    print("(sin PyMuPDF: el texto del PDF no se mira)")
bmp = bytes_de(os.path.join(salida, "previa-1.bmp")) or b""
pix = {bmp[i:i + 3] for i in range(54, len(bmp), 4)}
# BGR: fondo del código (246, 248, 250) y rejilla de las tablas y barra de las citas (223, 226, 229).
ok(bytes([250, 248, 246]) in pix and bytes([229, 226, 223]) in pix, "imprimir: en la vista previa, el fondo del código y la barra de la cita")
ok(k32.WaitForSingleObject(pi.hProcess, 0) != 0 and texto(e).startswith("# Imprimir"), "imprimir: Bloc sigue con el texto como estaba")
cerrar_sin_guardar(pi, v)

# --- M3 (1.4): la ventana, del tamaño que se quiera. Ninguna franja pasa del 25 % del alto; lo escondido vuelve. ---
WS_CAPTION, HTCAPTION, WM_NCHITTEST, VK_F3 = 0x00C00000, 2, 0x84, 0x72
u32.GetWindowLongW.restype = ctypes.c_long
tam = os.path.join(dir, "tamano.md")
with open(tam, "w", encoding="utf-8", newline="") as f:
    f.write("# Tamaño\r\n\r\nUn texto con la palabra aguja para buscarla.\r\n")
pi, v, e = abrir(tam)
barra_b = next(x for x in hijos(v) if clase(x) == "BlocBotones")
barra_e = next(x for x in hijos(v) if clase(x) == "BlocEstado")


def tamano(ancho, alto):
    u32.MoveWindow(v, 0, 0, ancho, alto, True)
    u32.SendMessageW(v, 0, 0, 0)


def con_titulo():
    return u32.GetWindowLongW(v, -16) & WS_CAPTION == WS_CAPTION


tamano(900, 700)
orden(v, "Edición", "Reemplazar")
time.sleep(0.5)   # el despliegue del panel, unos 120 ms
fila_b, dos_filas, fila_e = alto_de(barra_b), alto_de(panel_de(v)), alto_de(barra_e)
ok(con_titulo() and fila_b > 0 and dos_filas > 0 and fila_e > 0, "tamaño: a 900×700, todo a la vista (barra %d, panel %d, estado %d)" % (fila_b, dos_filas, fila_e))
tamano(260, 700)
ok(alto_de(barra_b) == fila_b, "tamaño: estrecha, la barra de botones sigue en una fila (los demás, en «»»)")
tamano(900, dos_filas * 4 - 8)
ok(0 < alto_de(panel_de(v)) < dos_filas, "tamaño: con el panel por encima del 25 %%, solo su fila de buscar (%d)" % alto_de(panel_de(v)))
tamano(900, 150)
ok(alto_de(e) >= 20, "tamaño: a 150 de alto, el texto sigue teniendo al menos una línea (%d)" % alto_de(e))
tamano(900, 80)
ok(not con_titulo(), "tamaño: a 80 de alto, sin barra de título")
ok(alto_de(barra_b) == 0 and alto_de(panel_de(v)) == 0 and alto_de(barra_e) == 0, "tamaño: y sin barra de botones, panel ni barra de estado")
origen = w.POINT(60, 2)
u32.ClientToScreen(v, ctypes.byref(origen))
ok(u32.SendMessageW(v, WM_NCHITTEST, 0, (origen.y & 0xFFFF) << 16 | (origen.x & 0xFFFF)) == HTCAPTION,
   "tamaño: sin barra de título, la franja de arriba mueve la ventana")
poner(casilla(panel_de(v), 10), "aguja")
u32.SendMessageW(e, EM_SETSEL, 0, 0)
u32.PostMessageW(e, WM_KEYDOWN, VK_F3, 0)
ok(esperar(lambda: seleccion(e)[1] - seleccion(e)[0] == 5, 3) is not None, "tamaño: con todo escondido, F3 sigue buscando")
tamano(50, 30)
r = w.RECT()
u32.GetWindowRect(v, ctypes.byref(r))
print("       (lo más pequeño que deja Windows: %d×%d)" % (r.right - r.left, r.bottom - r.top))
tamano(900, 700)
ok(con_titulo() and alto_de(barra_b) == fila_b and alto_de(panel_de(v)) == dos_filas and alto_de(barra_e) == fila_e,
   "tamaño: al agrandarla vuelve todo, la barra de título la primera")
cerrar_sin_guardar(pi, v)
with open(INI, encoding="utf-8") as f:
    ok("barra=1" in f.read(), "tamaño: lo elegido en «Ver» (la barra de estado) no ha cambiado")

# --- M10 (1.4): cada ventana nueva, del color contrario a la de Bloc que está delante. ---
u32.GetPropW.restype = ctypes.c_void_p
u32.FindWindowExW.restype = w.HWND
u32.FindWindowExW.argtypes = [w.HWND, w.HWND, w.LPCWSTR, w.LPCWSTR]


def modo(v):
    return {1: "claro", 2: "oscuro"}.get(u32.GetPropW(v, "BlocNoche") or 0, "sin modo")


def ventanas_seguidas(ini, n):
    with open(INI, "w", encoding="utf-8") as f:
        f.write(ini)
    abiertas = []
    for k in range(n):
        ruta = os.path.join(dir, "color-%d.txt" % k)
        with open(ruta, "w", encoding="utf-8") as f:
            f.write("ventana %d" % k)
        abiertas.append(abrir(ruta))
    modos = [modo(v) for _, v, _ in abiertas]
    for pi, v, _ in reversed(abiertas):
        cerrar_sin_guardar(pi, v)
    return modos


ok(ventanas_seguidas("noche=0\n", 3) == ["claro", "oscuro", "claro"], "color contrario: desde una clara, tres ventanas seguidas, alternando")
with open(INI, encoding="utf-8") as f:
    ok("noche=0" in f.read(), "color contrario: al cerrarlas, el modo guardado sigue siendo el de antes")
ok(ventanas_seguidas("noche=1\n", 2) == ["oscuro", "claro"], "color contrario: desde una oscura, la siguiente clara")
ok(ventanas_seguidas("noche=0\ncontrario=0\n", 2) == ["claro", "claro"], "color contrario: con la opción quitada, todas con el guardado")
ok(ventanas_seguidas("noche=1\n", 1) == ["oscuro"], "color contrario: sin otra ventana, el modo guardado")
t = time.perf_counter()
for _ in range(100):
    h = u32.FindWindowExW(None, None, "BlocVentana", None)
    u32.GetPropW(h, "BlocNoche")
ms = (time.perf_counter() - t) * 10
ok(ms < 1, "color contrario: saber el color de la otra ventana cuesta %.3f ms" % ms)

if len(sys.argv) > 3:   # una carpeta del NAS para probar el guardado en red
    ruta = os.path.join(sys.argv[3], "nas-prueba.md")
    with open(ruta, "wb") as f:
        f.write(b"\xef\xbb\xbf# Prueba\r\nl\xc3\xadnea\r\n")
    pi, v, e = abrir(ruta)
    teclear(e, "q")
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    u32.PostMessageW(dialogo(pi.dwProcessId), WM_COMMAND, IDYES, 0)
    ok(terminar(pi), "NAS: guarda y se cierra")
    with open(ruta, "rb") as f:
        ok(f.read() == b"\xef\xbb\xbfq# Prueba\r\nl\xc3\xadnea\r\n", "NAS: guarda con BOM y CRLF como el original")
    ok(not [n for n in os.listdir(sys.argv[3]) if n.endswith(".bloc-tmp")], "NAS: no deja temporales")
    os.remove(ruta)

u32.CloseDesktop(escritorio)
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
