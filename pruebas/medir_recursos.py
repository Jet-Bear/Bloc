# pyright: reportUndefinedVariable=false
# ruff: noqa: F821  (los nombres vienen de prueba_azar.py, cargado con exec)
"""Arranque, memoria, objetos de Windows y CPU en reposo de Bloc, en escritorio oculto y con su propio ini:
- arranque: de crear el proceso a que la ventana atiende con el texto pintado; el primero, tras copiar el .exe a un
  nombre nuevo, y la mediana de 5 más;
- memoria propia (private bytes) al abrir un .md de 50 KB y tras 10.000 ediciones (letras, retrocesos, Intro, negrita,
  viñetas, deshacer y rehacer);
- objetos GDI y USER al abrir y tras las ediciones: no pueden crecer;
- CPU en reposo durante 60 s: tiene que ser 0.
Uso: python medir_recursos.py <Bloc.exe> <carpeta> [ediciones] [segundos de reposo]"""
import ctypes, os, shutil, statistics, sys, threading, time

exe, dir = sys.argv[1:3]
N_EDICIONES = int(sys.argv[3]) if len(sys.argv) > 3 else 10_000
SEGUNDOS_REPOSO = int(sys.argv[4]) if len(sys.argv) > 4 else 60
sys.argv = ["x", exe, dir, "0", "1"]
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "prueba_azar.py"), encoding="utf-8") as f:
    fuente = f.read()
exec(compile(fuente[:fuente.index("hilo = threading.Thread")], "prueba_azar", "exec"))
DOC = SEMILLA_DOC + "\n"
MEDIANO = DOC * (50_000 // len(DOC.encode()) + 1)


class PMC(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("PageFaultCount", w.DWORD)] + [(n, ctypes.c_size_t) for n in (
        "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
        "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]


psapi = ctypes.windll.psapi
psapi.GetProcessMemoryInfo.argtypes = [w.HANDLE, ctypes.c_void_p, w.DWORD]
u32.GetGuiResources.argtypes = [w.HANDLE, w.DWORD]
k32.GetProcessTimes.argtypes = [w.HANDLE] + [ctypes.POINTER(w.FILETIME)] * 4


def memoria(p):
    m = PMC(); m.cb = ctypes.sizeof(PMC)
    psapi.GetProcessMemoryInfo(p, ctypes.byref(m), m.cb)
    return m.PrivateUsage // 1024


def objetos(p):
    return u32.GetGuiResources(p, 0), u32.GetGuiResources(p, 1)


def cpu(p):
    t = [w.FILETIME() for _ in range(4)]
    k32.GetProcessTimes(p, *[ctypes.byref(x) for x in t])
    return sum((x.dwHighDateTime << 32 | x.dwLowDateTime) for x in t[2:]) / 10_000   # ms de núcleo y de usuario


def abrir(esc, nombre, programa, ruta):
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = nombre; pi = PI()
    t0 = time.perf_counter()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (programa, ruta)), None, None, False, 0, None, None,
                       ctypes.byref(si), ctypes.byref(pi))
    v = None
    while not v:
        v = next((h for h in ventanas(esc, pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None); time.sleep(0.002)
    enviar(v, 0, segundos=60)
    u32.UpdateWindow(caja(v))
    return pi, v, (time.perf_counter() - t0) * 1000


def cerrar(pi):
    k32.TerminateProcess(pi.hProcess, 0); k32.WaitForSingleObject(pi.hProcess, 5000)


def trabajo():
    nombre = "BlocRecursos"
    esc = u32.CreateDesktopW(nombre, None, None, 0, 0x10000000, None); u32.SetThreadDesktop(esc)
    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=0\n")
    ruta = os.path.join(dir, "recursos.md")
    with open(ruta, "w", encoding="utf-8") as f:
        f.write(MEDIANO)
    copia = os.path.join(dir, "Bloc-%d.exe" % os.getpid())
    shutil.copyfile(exe, copia)
    pi, _, primero = abrir(esc, nombre, copia, ruta); cerrar(pi)
    os.remove(copia)
    otros = []
    for _ in range(5):
        pi, _, ms = abrir(esc, nombre, exe, ruta); otros.append(ms); cerrar(pi)
    print("arranque: primero tras copiar %.0f ms; mediana de 5 más %.0f ms" % (primero, statistics.median(otros)))
    print("tamaño: %d bytes" % os.path.getsize(exe))
    pi, v, _ = abrir(esc, nombre, exe, ruta)
    try:
        e = caja(v)
        time.sleep(1)
        m0, o0 = memoria(pi.hProcess), objetos(pi.hProcess)
        t = texto(e)
        medio = t.index("\r- ", len(t) // 2) + 3
        t0 = time.perf_counter()
        for i in range(N_EDICIONES):
            k = i % 10
            if k < 5:
                enviar(e, WM_CHAR, ord("x"))
            elif k < 7:
                enviar(e, WM_KEYDOWN, VK_BACK)
            elif k == 7:
                enviar(e, EM_SETSEL, medio, medio + 4); enviar(v, WM_COMMAND, NEGRITA)
            elif k == 8:
                enviar(v, WM_COMMAND, DESHACER)
            else:
                enviar(v, WM_COMMAND, REHACER); enviar(e, EM_SETSEL, medio + 8, medio + 8)
        enviar(v, 0)
        s = time.perf_counter() - t0
        time.sleep(1)
        m1, o1 = memoria(pi.hProcess), objetos(pi.hProcess)
        print("memoria propia: al abrir %d KB, tras %d ediciones %d KB (%.0f s)" % (m0, N_EDICIONES, m1, s))
        print("objetos GDI / USER: al abrir %d / %d, tras las ediciones %d / %d" % (*o0, *o1))
        c0 = cpu(pi.hProcess); time.sleep(SEGUNDOS_REPOSO); c1 = cpu(pi.hProcess)
        print("CPU en reposo: %.0f ms en %d s" % (c1 - c0, SEGUNDOS_REPOSO))
    finally:
        cerrar(pi)


h = threading.Thread(target=trabajo); h.start(); h.join()
