# pyright: reportUndefinedVariable=false
"""Cuánto tarda Bloc en atender cada tecla en un .md pequeño, uno de 50 KB y uno de 900 KB, con las marcas ocultas y a la
vista. Mide lo que tarda SendMessage en volver (Bloc ya ha leído, anotado y repintado). En escritorio oculto y con su
propio ini. Uso: python medir_teclas.py <Bloc.exe> <carpeta> [salida.csv]"""
import ctypes, os, statistics, sys, threading, time

exe, dir = sys.argv[1:3]
csv = sys.argv[3] if len(sys.argv) > 3 else None
sys.argv = ["x", exe, dir, "0", "1"]
fuente = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "prueba_azar.py"), encoding="utf-8").read()
exec(compile(fuente[:fuente.index("hilo = threading.Thread")], "prueba_azar", "exec"))
DOC = SEMILLA_DOC + PARTIDOS_DOC + "\n"  # noqa: F821
TAMANOS = [("pequeño", DOC), ("50 KB", DOC * (50_000 // len(DOC.encode()) + 1)), ("900 KB", DOC * (900_000 // len(DOC.encode()) + 1))]


def medir(e, v):
    t = texto(e)  # noqa: F821
    lista = "\r- viñeta uno"
    medio = (t.find(lista, len(t) // 2) if t.find(lista, len(t) // 2) >= 0 else t.index(lista)) + 1   # una lista, a media altura
    res = {}

    def tiempo(nombre, accion, n):
        ms = []
        for _ in range(n):
            t0 = time.perf_counter(); accion(); ms.append((time.perf_counter() - t0) * 1000)
        res[nombre] = ms

    enviar(e, EM_SETSEL, medio + 5, medio + 5)  # noqa: F821
    tiempo("letra", lambda: enviar(e, WM_CHAR, ord("x")), 60)  # noqa: F821
    tiempo("retroceso", lambda: enviar(e, WM_KEYDOWN, VK_BACK), 30)  # noqa: F821
    tiempo("intro", lambda: enviar(e, WM_KEYDOWN, VK_RETURN), 10)  # noqa: F821
    tiempo("negrita", lambda: (enviar(e, EM_SETSEL, medio + 2, medio + 8), enviar(v, WM_COMMAND, NEGRITA)), 10)  # noqa: F821
    tiempo("viñetas", lambda: (enviar(e, EM_SETSEL, medio, medio + 30), enviar(v, WM_COMMAND, LISTA)), 10)  # noqa: F821
    tiempo("mover cursor", lambda: enviar(e, EM_SETSEL, medio + 3, medio + 3), 30)  # noqa: F821
    return res


def trabajo():
    escritorio = u32.CreateDesktopW("BlocTeclas", None, None, 0, 0x10000000, None)  # noqa: F821
    u32.SetThreadDesktop(escritorio)  # noqa: F821
    filas = []
    for marcas in (0, 1):
        for nombre, doc in TAMANOS:
            with open(INI, "w") as f:  # noqa: F821
                f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=%d\n" % marcas)
            ruta = os.path.join(dir, "teclas.md")
            with open(ruta, "w", encoding="utf-8", newline="\n") as f:
                f.write(doc)
            si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = "BlocTeclas"; pi = PI()  # noqa: F821
            k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,  # noqa: F821
                               ctypes.byref(si), ctypes.byref(pi))
            u32.WaitForInputIdle(pi.hProcess, 30000)  # noqa: F821
            try:
                v = None
                for _ in range(400):
                    v = next((h for h in ventanas(escritorio, pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None)  # noqa: F821
                    if v:
                        break
                    time.sleep(0.05)
                e = caja(v)  # noqa: F821
                enviar(v, 0, segundos=60)  # noqa: F821
                for accion, ms in medir(e, v).items():
                    ms.sort()
                    fila = (nombre, "a la vista" if marcas else "ocultas", accion, statistics.median(ms), ms[int(len(ms) * 0.9) - 1], ms[-1])
                    filas.append(fila)
                    print("%-8s %-11s %-13s mediana %7.2f  p90 %7.2f  máx %7.2f" % fila)
            finally:
                k32.TerminateProcess(pi.hProcess, 0)  # noqa: F821
                k32.WaitForSingleObject(pi.hProcess, 5000)  # noqa: F821
    if csv:
        with open(csv, "w", encoding="utf-8") as f:
            f.write("doc;marcas;accion;mediana_ms;p90_ms;max_ms\n")
            for fila in filas:
                f.write("%s;%s;%s;%.2f;%.2f;%.2f\n" % fila)


h = threading.Thread(target=trabajo)
h.start(); h.join()
