"""Estabilidad de Bloc.exe en un escritorio invisible: archivos raros, teclas muertas del teclado español y emojis,
apagar Windows con cambios sin guardar y el cursor que se desliza al escribir.
Uso: python prueba_estabilidad.py <Bloc.exe> <carpeta de trabajo>"""
import ctypes, ctypes.wintypes as w, os, sys, threading, time

k32, u32 = ctypes.windll.kernel32, ctypes.windll.user32
u32.CreateDesktopW.restype = w.HANDLE
u32.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u32.SendMessageW.restype = w.LPARAM
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u32.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]
u32.GetKeyboardLayout.restype = ctypes.c_void_p
u32.VkKeyScanExW.argtypes = [w.WCHAR, ctypes.c_void_p]
u32.MapVirtualKeyExW.argtypes = [w.UINT, w.UINT, ctypes.c_void_p]
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
WM_GETTEXT, WM_GETTEXTLENGTH, WM_CLOSE, WM_COMMAND, WM_KEYDOWN, WM_KEYUP, WM_CHAR = 0x0D, 0x0E, 0x10, 0x111, 0x100, 0x101, 0x102
WM_QUERYENDSESSION, WM_ENDSESSION, EM_SETSEL, IDYES, IDNO = 0x11, 0x16, 0xB1, 6, 7


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("lpTitle", w.LPWSTR), ("dwX", w.DWORD),
                ("dwY", w.DWORD), ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD), ("dwYCountChars", w.DWORD),
                ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD), ("wShowWindow", w.WORD), ("cbReserved2", w.WORD),
                ("lpReserved2", ctypes.c_void_p), ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


ESCRITORIO = "BlocEstabilidad"
escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
exe, dir = sys.argv[1], sys.argv[2]
os.environ["BLOC_SIN_INSTALAR"] = "1"
os.environ["APPDATA"] = dir   # errores.log va a la carpeta de la prueba, nunca a la del usuario
LOG = os.path.join(dir, "Bloc", "errores.log")
INI = os.environ["BLOC_INI"] = os.path.join(dir, "bloc-estabilidad.ini")
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")
fallos = 0


def ok(bien, que):
    global fallos
    print(("ok     " if bien else "FALLO  ") + que, flush=True)
    fallos += 0 if bien else 1


def clase(h):
    b = ctypes.create_unicode_buffer(256)
    u32.GetClassNameW(h, b, 256)
    return b.value


def texto(h):
    r = ctypes.c_size_t()
    if not u32.SendMessageTimeoutW(h, WM_GETTEXTLENGTH, 0, 0, 2, 5000, ctypes.byref(r)):
        return None
    b = ctypes.create_unicode_buffer(r.value + 1)
    u32.SendMessageW(h, WM_GETTEXT, r.value + 1, ctypes.addressof(b))
    # Los saltos blandos de un .md van en el control como U+2005 (md::JUNTO): el texto del archivo lleva el salto.
    return b.value.replace(" ", "\r\n")


def ventanas(pid):
    res = []

    def cb(h, _):
        p = w.DWORD()
        u32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and u32.IsWindowVisible(h):
            res.append(h)
        return True
    u32.EnumDesktopWindows(escritorio, ENUM(cb), 0)
    return res


def esperar(cond, segundos=8):
    fin = time.time() + segundos
    while time.time() < fin:
        r = cond()
        if r:
            return r
        time.sleep(0.05)
    return None


def abrir(ruta):
    si = SI()
    si.cb = ctypes.sizeof(SI)
    si.lpDesktop = ESCRITORIO
    pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,
                       ctypes.byref(si), ctypes.byref(pi))
    u32.WaitForInputIdle(pi.hProcess, 15000)
    v = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "BlocVentana"), None), 15)
    hijos = []
    u32.EnumChildWindows(v, ENUM(lambda x, _: hijos.append(x) or True), 0)
    return pi, v, next((x for x in hijos if "EDIT" in clase(x)), None)


def dialogo(pid, segundos=3):
    return esperar(lambda: next((h for h in ventanas(pid) if clase(h) == "#32770"), None), segundos)


def cerrar_sin_guardar(pi, v):
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    d = dialogo(pi.dwProcessId)
    if d:
        u32.PostMessageW(d, WM_COMMAND, IDNO, 0)
    acabado = k32.WaitForSingleObject(pi.hProcess, 6000) == 0
    if not acabado:
        k32.TerminateProcess(pi.hProcess, 1)
    return acabado


def sin_errores(que):
    if os.path.exists(LOG):
        with open(LOG, encoding="utf-8", errors="replace") as f:
            ok(False, "%s: errores.log dice %r" % (que, f.read()[-300:]))
        os.remove(LOG)


def archivos_raros():
    raros = [
        ("vacio.md", b"", lambda t: t == ""),
        ("una-linea-1mb.md", b"a" * 1_000_000, lambda t: len(t) == 1_000_000),
        ("binario.bin", bytes(range(256)) * 64, lambda t: t is not None),
        ("nulos.txt", b"a\0b\0c", lambda t: t == "a b c"),
        ("utf16-suelta.txt", b"\xff\xfe" + "x".encode("utf-16-le") + b"\x00\xd8" + "y".encode("utf-16-le"), lambda t: t is not None and t[0] == "x" and t[-1] == "y"),
        ("saltos-mezclados.md", b"uno\r\ndos\ntres\rcuatro", lambda t: t is not None and t.replace("\r\n", "\n").split("\n") == ["uno", "dos", "tres", "cuatro"]),
        ("bom8.md", b"\xef\xbb\xbf# T\xc3\xadtulo", lambda t: t == "# Título"),
        ("bom16le.txt", b"\xff\xfe" + "Ñandú".encode("utf-16-le"), lambda t: t == "Ñandú"),
        ("bom16be.txt", b"\xfe\xff" + "Ñandú".encode("utf-16-be"), lambda t: t == "Ñandú"),
        ("tabla-rara.md", "| a |\n|---|---|---|\n| 1 | 2 | 3 | 4 |\n|\n| \\| | `|` |\n".encode(), lambda t: t is not None and t.count("|") >= 10),
    ]
    for nombre, datos, bien in raros:
        ruta = os.path.join(dir, nombre)
        with open(ruta, "wb") as f:
            f.write(datos)
        pi, v, e = abrir(ruta)
        t = texto(e) if e else None
        ok(e is not None and bien(t), "archivo raro %s: se abre con su texto" % nombre)
        if e:
            u32.SendMessageW(e, EM_SETSEL, 0, 0)
            u32.SendMessageW(e, WM_CHAR, ord("z"), 0)
            ok(texto(e) is not None, "archivo raro %s: se puede escribir" % nombre)
        ok(v is not None and cerrar_sin_guardar(pi, v), "archivo raro %s: se cierra bien" % nombre)
        with open(ruta, "rb") as f:
            ok(f.read() == datos, "archivo raro %s: sin guardar, no se ha tocado" % nombre)
        sin_errores(nombre)


def tecla(e, vk, hkl):
    escan = u32.MapVirtualKeyExW(vk, 0, hkl)
    u32.PostMessageW(e, WM_KEYDOWN, vk, 1 | (escan << 16))
    u32.PostMessageW(e, WM_KEYUP, vk, 1 | (escan << 16) | (3 << 30))


def teclado():
    ruta = os.path.join(dir, "teclas.md")
    with open(ruta, "w", encoding="utf-8") as f:
        f.write("")
    pi, v, e = abrir(ruta)
    hilo = u32.GetWindowThreadProcessId(e, None)
    hkl = u32.GetKeyboardLayout(hilo)
    agudo = u32.VkKeyScanExW("´", hkl)
    if (hkl or 0) & 0xFFFF in (0x040A, 0x0C0A) and agudo != -1 and not agudo & 0x700:
        # Como del teclado: Bloc traduce la tecla muerta y la letra (TranslateMessage) en su propio bucle.
        tecla(e, agudo & 0xFF, hkl)
        tecla(e, 0x41, hkl)
        esperar(lambda: texto(e), 3)
        ok(texto(e) == "á", "teclado español: ´ y luego a escribe «á» (hay %r)" % texto(e))
    else:
        print("saltada: el teclado de Bloc no es el español (%#x)" % ((hkl or 0) & 0xFFFF))
    u32.SendMessageW(e, EM_SETSEL, 0, -1)
    for c in "x😀":
        for unidad in c.encode("utf-16-le").hex(" ", 2).split():
            u32.SendMessageW(e, WM_CHAR, int(unidad[2:] + unidad[:2], 16), 0)
    u32.SendMessageW(e, WM_KEYDOWN, 8, 0)
    u32.SendMessageW(e, WM_CHAR, 8, 0)
    ok(texto(e) == "x", "emoji: un Retroceso lo quita entero (hay %r)" % texto(e))
    for c in "é":
        u32.SendMessageW(e, WM_CHAR, ord(c), 0)
    u32.SendMessageW(e, EM_SETSEL, 0, -1)
    u32.SendMessageW(e, WM_KEYDOWN, 0x2E, 0)
    ok(texto(e) == "", "acento combinado: seleccionado y Supr, no queda nada (hay %r)" % texto(e))
    ok(cerrar_sin_guardar(pi, v), "teclado: se cierra bien")
    sin_errores("teclado")


class GUI(ctypes.Structure):
    _fields_ = [("cbSize", w.DWORD), ("flags", w.DWORD), ("hwndActive", w.HWND), ("hwndFocus", w.HWND), ("hwndCapture", w.HWND),
                ("hwndMenuOwner", w.HWND), ("hwndMoveSize", w.HWND), ("hwndCaret", w.HWND), ("rcCaret", w.RECT)]


def cursor_de(e):
    g = GUI()
    g.cbSize = ctypes.sizeof(GUI)
    u32.GetGUIThreadInfo(u32.GetWindowThreadProcessId(e, None), ctypes.byref(g))
    return g.rcCaret.left, g.rcCaret.top


def cursor_suave():
    ruta = os.path.join(dir, "cursor.md")
    with open(ruta, "w", encoding="utf-8") as f:
        f.write("Hola mundo")
    pi, v, e = abrir(ruta)
    u32.SendMessageW(e, EM_SETSEL, 4, 4)
    x0 = cursor_de(e)[0]
    vistos = set()
    u32.SendMessageW(e, WM_CHAR, ord("m"), 0)
    fin = time.perf_counter() + 0.25
    while time.perf_counter() < fin:
        vistos.add(cursor_de(e)[0])
        time.sleep(0.003)
    deslizado = cursor_de(e)
    u32.SendMessageW(e, EM_SETSEL, 5, 5)   # RichEdit vuelve a poner el cursor donde él dice
    bueno = cursor_de(e)
    ok(deslizado == bueno, "cursor suave: acaba donde lo pone RichEdit (%s y %s)" % (deslizado, bueno))
    ok(any(x0 < x < bueno[0] for x in vistos), "cursor suave: pasa por en medio al escribir (de %d a %d: %s)" % (x0, bueno[0], sorted(vistos)))
    u32.SendMessageW(e, WM_KEYDOWN, 8, 0)
    time.sleep(0.25)
    deslizado = cursor_de(e)
    u32.SendMessageW(e, EM_SETSEL, 4, 4)
    ok(deslizado == cursor_de(e) and texto(e) == "Hola mundo", "cursor suave: al borrar, también acaba en su sitio")
    for c in "abcdefghij":   # deprisa: cada letra empieza otro deslizamiento desde donde iba el anterior
        u32.SendMessageW(e, WM_CHAR, ord(c), 0)
    time.sleep(0.25)
    deslizado = cursor_de(e)
    u32.SendMessageW(e, EM_SETSEL, 14, 14)
    ok(deslizado == cursor_de(e), "cursor suave: escribiendo deprisa acaba en su sitio")
    ok(cerrar_sin_guardar(pi, v), "cursor suave: se cierra bien")
    sin_errores("cursor suave")


def apagar():
    ruta = os.path.join(dir, "apagar.md")
    with open(ruta, "w", encoding="utf-8", newline="") as f:
        f.write("texto")
    pi, v, e = abrir(ruta)
    r = ctypes.c_size_t()
    u32.SendMessageTimeoutW(v, WM_QUERYENDSESSION, 0, 0, 2, 5000, ctypes.byref(r))
    ok(r.value == 1, "apagar sin cambios: deja apagar sin preguntar")
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    u32.SendMessageW(e, WM_CHAR, ord("x"), 0)
    respuesta = []
    hilo = threading.Thread(target=lambda: respuesta.append(
        (u32.SendMessageTimeoutW(v, WM_QUERYENDSESSION, 0, 0, 0, 15000, ctypes.byref(r)), r.value)))
    hilo.start()
    # Desde la 1.2 (K10), al apagar no pregunta: lo escrito va a la nota de la próxima vez, junto al ini.
    ok(dialogo(pi.dwProcessId, 2) is None, "apagar con cambios: no pregunta")
    hilo.join(16)
    ok(respuesta and respuesta[0][1] == 1, "apagar con cambios: deja apagar")
    u32.SendMessageTimeoutW(v, WM_ENDSESSION, 1, 0, 2, 5000, ctypes.byref(r))
    with open(ruta, encoding="utf-8") as f:
        ok(f.read() == "texto", "apagar con cambios: el archivo no cambia")
    nota = os.path.join(os.path.dirname(INI), "nota.bloc")
    datos = b""
    if os.path.exists(nota):
        with open(nota, "rb") as f:
            datos = f.read()
        os.remove(nota)
    ok(b"xtexto" in datos or "xtexto".encode("utf-16-le") in datos, "apagar con cambios: lo escrito está en la nota")
    k32.TerminateProcess(pi.hProcess, 0)
    sin_errores("apagar")


def principal():
    u32.SetThreadDesktop(escritorio)
    archivos_raros()
    teclado()
    cursor_suave()
    apagar()


h = threading.Thread(target=principal)
h.start()
h.join()
u32.CloseDesktop(escritorio)
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
