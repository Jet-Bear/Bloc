# pyright: reportUndefinedVariable=false
"""Fluidez de Bloc más allá de las teclas: abrir, rueda del ratón (una muesca, ráfagas y a ritmo de 60 por segundo),
saltar con la barra de desplazamiento, redimensionar y las opciones de Ver. En escritorio oculto y con su propio ini.
Uso: python medir_fluidez.py <Bloc.exe> <carpeta> [salida.csv]

La rueda: RichEdit anima el desplazamiento de cada muesca. «tras» es lo que sigue moviéndose la vista después de que
Bloc haya atendido la última muesca: con una ráfaga, si es mucho, la rueda «tiene un límite de velocidad»."""
import ctypes, os, statistics, sys, threading, time

exe, dir = sys.argv[1:3]
csv = sys.argv[3] if len(sys.argv) > 3 else None
sys.argv = ["x", exe, dir, "0", "1"]
fuente = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "prueba_azar.py"), encoding="utf-8").read()
exec(compile(fuente[:fuente.index("hilo = threading.Thread")], "prueba_azar", "exec"))
DOC = SEMILLA_DOC + PARTIDOS_DOC + "\n"  # noqa: F821
GRANDE = DOC * (900_000 // len(DOC.encode()) + 1)
MEDIANO = DOC * (50_000 // len(DOC.encode()) + 1)
DOCS = [("md 50 KB", "md", MEDIANO), ("md 900 KB", "md", GRANDE), ("txt 900 KB", "txt", GRANDE)]
WM_MOUSEWHEEL, WM_NULL = 0x20A, 0
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]  # noqa: F821
u32.GetScrollInfo.argtypes = [w.HWND, ctypes.c_int, ctypes.c_void_p]  # noqa: F821


def pos_v(e):
    si = SCROLLINFO(ctypes.sizeof(SCROLLINFO), 0x4)   # SIF_POS  # noqa: F821
    u32.GetScrollInfo(e, 1, ctypes.byref(si))  # noqa: F821
    return si.nPos


def quieta(e, desde, limite=1.5):
    """Espera a que la vista deje de moverse (300 ms sin cambios). Devuelve cuándo cambió por última vez (s desde
    `desde`) y dónde ha quedado."""
    ultima, p, cambio = time.perf_counter(), pos_v(e), desde
    while time.perf_counter() - ultima < 0.3 and time.perf_counter() - desde < limite:
        time.sleep(0.002)
        q = pos_v(e)
        if q != p:
            p, cambio, ultima = q, time.perf_counter(), time.perf_counter()
    return cambio - desde, p


def rueda(delta):
    return (delta & 0xFFFF) << 16


def medir(e, v, proceso, remoto):
    res = {}

    def ms(nombre, accion, n=1):
        for _ in range(n):
            t0 = time.perf_counter(); accion(); res.setdefault(nombre, []).append((time.perf_counter() - t0) * 1000)

    def ir(y):
        k32.WriteProcessMemory(proceso, remoto, (ctypes.c_long * 2)(0, y), 8, None)  # noqa: F821
        enviar(e, EM_SETSCROLLPOS, 0, remoto)  # noqa: F821

    def pintar():
        u32.RedrawWindow(v, None, None, 0x100 | 0x80)   # RDW_UPDATENOW | RDW_ALLCHILDREN  # noqa: F821
        enviar(v, WM_NULL, segundos=180)  # noqa: F821

    centro = ((400 & 0xFFFF) << 16) | 300
    si = SCROLLINFO(ctypes.sizeof(SCROLLINFO), 0x17)  # noqa: F821
    u32.GetScrollInfo(e, 1, ctypes.byref(si))  # noqa: F821
    y0 = min(20000, (si.nMax - si.nPage) // 3)
    # Una muesca: lo que tarda en atenderla y lo que sigue moviéndose después.
    for _ in range(8):
        ir(y0); pintar()
        t0 = time.perf_counter()
        enviar(e, WM_MOUSEWHEEL, rueda(-120), centro)  # noqa: F821
        res.setdefault("rueda 1: atender", []).append((time.perf_counter() - t0) * 1000)
        p0 = pos_v(e)
        t1 = time.perf_counter()
        tras, p = quieta(e, t1)
        res.setdefault("rueda 1: tras", []).append(tras * 1000)
        res.setdefault("rueda 1: px", []).append(p - y0)
        res.setdefault("rueda 1: px al volver", []).append(p0 - y0)
    # Ráfaga: 15 muescas de golpe, como al girar rápido.
    for _ in range(4):
        ir(y0); pintar()
        t0 = time.perf_counter()
        for _ in range(15):
            u32.PostMessageW(e, WM_MOUSEWHEEL, rueda(-120), centro)  # noqa: F821
        enviar(e, WM_NULL)  # noqa: F821
        atendidas = time.perf_counter()
        tras, p = quieta(e, atendidas, 3)
        res.setdefault("ráfaga 15: atender", []).append((atendidas - t0) * 1000)
        res.setdefault("ráfaga 15: tras", []).append(tras * 1000)
        res.setdefault("ráfaga 15: px", []).append(p - y0)
    # A ritmo: una muesca cada 16 ms (girar seguido), 30 muescas.
    for _ in range(2):
        ir(y0); pintar()
        for _ in range(30):
            t = time.perf_counter()
            enviar(e, WM_MOUSEWHEEL, rueda(-120), centro)  # noqa: F821
            resto = 0.016 - (time.perf_counter() - t)
            if resto > 0:
                time.sleep(resto)
        fin = time.perf_counter()
        tras, p = quieta(e, fin, 3)
        res.setdefault("ritmo 30: tras", []).append(tras * 1000)
        res.setdefault("ritmo 30: px", []).append(p - y0)
    # ¿Da RichEdit el desplazamiento en 32 bits? (la ayuda de EM_GETSCROLLPOS dice que en 16)
    ir(si.nMax - si.nPage)
    k32.WriteProcessMemory(proceso, remoto, (ctypes.c_long * 2)(0, 0), 8, None)  # noqa: F821
    enviar(e, 0x4DD, 0, remoto)   # EM_GETSCROLLPOS  # noqa: F821
    leido = (ctypes.c_long * 2)()
    k32.ReadProcessMemory(proceso, remoto, leido, 8, None)  # noqa: F821
    print("    alto %d, abajo del todo: EM_GETSCROLLPOS %d, barra %d" % (si.nMax, leido[1], pos_v(e)))
    # Saltar con la barra: 20 sitios del documento, pintando cada uno.
    for k in range(20):
        y = (si.nMax - si.nPage) * ((k * 7) % 20) // 19
        ms("barra: saltar y pintar", lambda: (ir(max(0, y)), pintar()))
    ir(0); pintar()
    # Redimensionar, con el ajuste de línea puesto.
    for k in range(12):
        ancho = 1000 if k % 2 else 760
        ms("redimensionar", lambda: (u32.MoveWindow(v, 0, 0, ancho, 800, True), pintar()))  # noqa: F821
    u32.MoveWindow(v, 0, 0, 1000, 800, True); pintar()  # noqa: F821
    for nombre, id, n in (("ajuste", AJUSTE, 4), ("acercar", ACERCAR, 3), ("alejar", ALEJAR, 3), ("marcas", MARCAS, 2), ("noche", NOCHE, 2)):  # noqa: F821
        try:
            ms(nombre, lambda: (enviar(v, WM_COMMAND, id, segundos=180), pintar()), n)
        except Colgado:  # noqa: F821
            res.setdefault(nombre, []).append(180000)
            break
    ms("ir al final", lambda: (enviar(e, EM_SETSEL, 10**8, 10**8), enviar(e, 0xB7), pintar()), 3)   # EM_SCROLLCARET  # noqa: F821
    return res


def trabajo():
    escritorio = u32.CreateDesktopW("BlocFluidez", None, None, 0, 0x10000000, None)  # noqa: F821
    u32.SetThreadDesktop(escritorio)  # noqa: F821
    filas = []
    for nombre, ext, doc in DOCS:
        with open(INI, "w") as f:  # noqa: F821
            f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=0\n")
        ruta = os.path.join(dir, "fluidez." + ext)
        with open(ruta, "w", encoding="utf-8", newline="\n") as f:
            f.write(doc)
        si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = "BlocFluidez"; pi = PI()  # noqa: F821
        t0 = time.perf_counter()
        k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,  # noqa: F821
                           ctypes.byref(si), ctypes.byref(pi))
        try:
            v = None
            while not v and time.perf_counter() - t0 < 30:
                v = next((h for h in ventanas(escritorio, pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None)  # noqa: F821
                time.sleep(0.002)
            e = caja(v)  # noqa: F821
            enviar(v, 0, segundos=60)  # noqa: F821
            abrir = (time.perf_counter() - t0) * 1000
            u32.MoveWindow(v, 0, 0, 1000, 800, True)  # noqa: F821
            proceso = k32.OpenProcess(0x0008 | 0x0010 | 0x0020, False, pi.dwProcessId)  # noqa: F821
            remoto = k32.VirtualAllocEx(proceso, None, 4096, 0x3000, 0x04)  # noqa: F821
            res = {"abrir": [abrir]}
            res.update(medir(e, v, proceso, remoto))
            for accion, ms in res.items():
                ms = sorted(ms)
                fila = (nombre, accion, statistics.median(ms), ms[-1])
                filas.append(fila)
                print("%-11s %-24s mediana %8.1f  máx %8.1f" % fila, flush=True)
        finally:
            k32.TerminateProcess(pi.hProcess, 0)  # noqa: F821
            k32.WaitForSingleObject(pi.hProcess, 5000)  # noqa: F821
    if csv:
        with open(csv, "w", encoding="utf-8") as f:
            f.write("doc;accion;mediana;max\n")
            for fila in filas:
                f.write("%s;%s;%.1f;%.1f\n" % fila)


h = threading.Thread(target=trabajo)
h.start(); h.join()
