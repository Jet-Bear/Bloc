# pyright: reportUndefinedVariable=false
# ruff: noqa: F821  (los nombres vienen de prueba_azar.py, cargado con exec)
"""¿Se ven varias barras del cursor a la vez al escribir o borrar deprisa? Escribe y borra ráfagas de «o» (una letra
sin trazos verticales) en distintos sitios de un .md y copia la ventana sin parar mientras Bloc las atiende. En cada
copia cuenta las rayas verticales oscuras de casi el alto de la línea, que solo pueden ser cursores.
Uso: python medir_cursor.py <Bloc.exe> <carpeta>"""
import ctypes, os, sys, threading, time

exe, dir = sys.argv[1:3]
sys.argv = ["x", exe, dir, "0", "1"]
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "prueba_azar.py"), encoding="utf-8") as f:
    fuente = f.read()
exec(compile(fuente[:fuente.index("hilo = threading.Thread")], "prueba_azar", "exec"))
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
DOC = "Un párrafo normal con texto\n\n- elemento de lista\n- otro elemento\n\n**negrita** y *cursiva* al final\n"
SITIOS = [("párrafo", "texto"), ("lista", "otro elemento"), ("negrita", "cursiva*")]


def copia(h):
    """La ventana tal como está, sin pedirle que se pinte (así se ve el cursor y lo que haya dejado a medias)."""
    r = w.RECT(); u32.GetClientRect(h, ctypes.byref(r))
    ancho, alto = r.right, r.bottom
    dc = u32.GetDC(h); m = g32.CreateCompatibleDC(dc); b = g32.CreateCompatibleBitmap(dc, ancho, alto)
    viejo = g32.SelectObject(m, b)
    g32.BitBlt(m, 0, 0, ancho, alto, dc, 0, 0, 0x00CC0020)
    bi = BIH(ctypes.sizeof(BIH), ancho, -alto, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(ancho * alto * 4)
    g32.SelectObject(m, viejo)
    g32.GetDIBits(m, b, 0, alto, buf, ctypes.byref(bi), 0)
    g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(h, dc)
    return ancho, alto, buf.raw


def barras(c, alto_linea):
    """Cuántas rayas verticales oscuras de al menos el 85 % del alto de la línea hay."""
    import numpy as np
    ancho, alto, d = c
    oscuro = np.frombuffer(d, np.uint8).reshape(alto, ancho, 4)[:, :, 1] < 90
    largo = int(alto_linea * 0.85)
    # Una columna tiene raya si alguna ventana de `largo` píxeles seguidos es toda oscura.
    suma = np.vstack([np.zeros((1, ancho), int), np.cumsum(oscuro, axis=0)])
    columnas = ((suma[largo:] - suma[:-largo]) == largo).any(axis=0)
    return int(np.sum(columnas[1:] & ~columnas[:-1]) + columnas[0])


def rafaga(v, e, mensajes, alto_linea):
    for m, wp in mensajes:
        u32.PostMessageW(e, m, wp, 0)
    copias, fin = [], time.time() + 0.6
    while time.time() < fin:
        copias.append(copia(e))
    enviar(v, 0)
    cuantas = [barras(c, alto_linea) for c in copias]
    peor = max(range(len(copias)), key=lambda i: cuantas[i])
    if os.environ.get("CURSOR_GUARDAR"):
        guardar_png(copias[peor], os.path.join(dir, "cursor-%d.png" % len(os.listdir(dir))))
    return cuantas[peor], sum(k > 1 for k in cuantas), len(cuantas)


def trabajo():
    nombre = "BlocCursor"
    esc = u32.CreateDesktopW(nombre, None, None, 0, 0x10000000, None); u32.SetThreadDesktop(esc)
    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=0\n")
    ruta = os.path.join(dir, "cursor." + os.environ.get("CURSOR_EXT", "md"))
    with open(ruta, "w", encoding="utf-8") as f:
        f.write(DOC)
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = nombre; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,
                       ctypes.byref(si), ctypes.byref(pi))
    try:
        v = None
        while not v:
            v = next((h for h in ventanas(esc, pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None); time.sleep(0.02)
        u32.MoveWindow(v, 0, 0, 500, 260, True)
        e = caja(v); enviar(v, 0); time.sleep(0.3)
        u32.SendMessageW(e, 0x0007, 0, 0)   # WM_SETFOCUS: el control crea y enseña su cursor
        # El alto de una línea, con la posición de dos líneas pedida a Bloc en memoria suya.
        k32.VirtualAllocEx.restype = ctypes.c_void_p
        k32.VirtualAllocEx.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_size_t, w.DWORD, w.DWORD]
        k32.ReadProcessMemory.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
        u32.SendMessageW.argtypes = [w.HWND, w.UINT, ctypes.c_size_t, ctypes.c_ssize_t]
        m = k32.VirtualAllocEx(pi.hProcess, None, 8, 0x3000, 4)
        p = w.POINT()

        def y_de(c):
            u32.SendMessageW(e, 0xD6, m, c)
            k32.ReadProcessMemory(pi.hProcess, m, ctypes.byref(p), 8, None)
            return p.y
        t = texto(e)
        alto_linea = y_de(t.index("\r") + 1) - y_de(0)
        print("alto de línea: %d px" % alto_linea)
        for sitio, tras in SITIOS:
            t = texto(e)
            c = t.index(tras) + len(tras)
            enviar(e, EM_SETSEL, c, c); enviar(v, 0); time.sleep(0.6)
            escribir = rafaga(v, e, [(WM_CHAR, ord("o"))] * 25, alto_linea)
            borrar = rafaga(v, e, [(WM_KEYDOWN, VK_BACK)] * 25, alto_linea)
            print("%-8s escribir: hasta %d barras a la vez, %d de %d copias con más de una · borrar: hasta %d, %d de %d"
                  % (sitio, escribir[0], escribir[1], escribir[2], borrar[0], borrar[1], borrar[2]))
    finally:
        k32.TerminateProcess(pi.hProcess, 0); k32.WaitForSingleObject(pi.hProcess, 5000)


h = threading.Thread(target=trabajo); h.start(); h.join()
