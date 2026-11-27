"""M3 (1.4): Bloc a varios tamaños, del normal al más pequeño que deja Windows, en claro y en oscuro, con el panel de
reemplazar abierto: cada franja se esconde cuando pasaría del 25 % del alto. En un escritorio oculto.
Uso: python capturas_tamanos.py <Bloc.exe> <carpeta de trabajo vacía> <carpeta de las imágenes>"""
import ctypes
import ctypes.wintypes as w
import os
import sys
import threading
import time

from PIL import Image

k32, u32, g32 = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.gdi32
for f, res, args in (
        (u32.CreateDesktopW, w.HANDLE, [w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.c_void_p]),
        (u32.SetThreadDesktop, w.BOOL, [w.HANDLE]),
        (u32.GetDC, w.HDC, [w.HWND]), (u32.GetWindowDC, w.HDC, [w.HWND]), (u32.ReleaseDC, ctypes.c_int, [w.HWND, w.HDC]),
        (g32.CreateCompatibleDC, w.HDC, [w.HDC]), (g32.CreateCompatibleBitmap, w.HBITMAP, [w.HDC, ctypes.c_int, ctypes.c_int]),
        (g32.SelectObject, w.HGDIOBJ, [w.HDC, w.HGDIOBJ]), (g32.DeleteObject, w.BOOL, [w.HGDIOBJ]), (g32.DeleteDC, w.BOOL, [w.HDC]),
        (g32.BitBlt, w.BOOL, [w.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.HDC, ctypes.c_int, ctypes.c_int, w.DWORD]),
        (g32.GetDIBits, ctypes.c_int, [w.HDC, w.HBITMAP, w.UINT, w.UINT, ctypes.c_void_p, ctypes.c_void_p, w.UINT]),
        (u32.RedrawWindow, w.BOOL, [w.HWND, ctypes.c_void_p, ctypes.c_void_p, w.UINT]),
        (u32.GetClientRect, w.BOOL, [w.HWND, ctypes.POINTER(w.RECT)]),
        (u32.MoveWindow, w.BOOL, [w.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.BOOL]),
        (u32.SendMessageW, w.LPARAM, [w.HWND, w.UINT, w.WPARAM, w.LPARAM]),
        (k32.TerminateProcess, w.BOOL, [w.HANDLE, w.UINT])):
    f.restype, f.argtypes = res, args
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
WM_COMMAND, WM_CLOSE, EM_SETSEL = 0x111, 0x10, 0xB1
REEMPLAZAR = 1020
ANCHO, ALTO = 900, 620
TAMANOS = [(900, 620), (600, 420), (420, 300), (320, 200), (260, 150), (240, 110), (200, 80), (60, 30)]

NOTAS = """# Weekend trip

A short plan for Saturday. **Bold**, *italic* and `code` look like this, and the marks stay hidden.

## To do

- [x] Book the train
- [x] Find a hotel near the station
- [ ] Pack the camera
- [ ] Print the tickets

## Budget

| Item | Cost |
|---|---:|
| Train | 48 |
| Hotel | 120 |
| Food | 60 |

> Leave before nine to miss the traffic.

```python
def total(costs):
    return sum(costs)
```

Timetable: [trains on Saturday](https://example.com/timetable)
"""


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


class BIH(ctypes.Structure):
    _fields_ = [("biSize", w.DWORD), ("biWidth", w.LONG), ("biHeight", w.LONG), ("biPlanes", w.WORD), ("biBitCount", w.WORD),
                ("biCompression", w.DWORD), ("biSizeImage", w.DWORD), ("biXPelsPerMeter", w.LONG), ("biYPelsPerMeter", w.LONG),
                ("biClrUsed", w.DWORD), ("biClrImportant", w.DWORD)]


exe, dir, salida = sys.argv[1:4]
ESCRITORIO = "BlocCapturasTamanos"
os.environ["BLOC_SIN_INSTALAR"] = "1"
os.environ["APPDATA"] = dir
os.environ["BLOC_INI"] = INI = os.path.join(dir, "bloc.ini")
os.makedirs(salida, exist_ok=True)


def clase(h):
    b = ctypes.create_unicode_buffer(256); u32.GetClassNameW(h, b, 256); return b.value


def ventana(escritorio, pid):
    res = []
    def cb(h, _):
        p = w.DWORD(); u32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and u32.IsWindowVisible(h) and clase(h) != "#32770":
            res.append(h)
        return True
    u32.EnumDesktopWindows(escritorio, ENUM(cb), 0)
    return res[0] if res else None


def hijo(h, que, exacta=False):
    res = []
    def cb(x, _):
        if clase(x).upper() == que if exacta else que in clase(x).upper():
            res.append(x)
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res[0] if res else None


class MENUBARINFO(ctypes.Structure):
    _fields_ = [("cbSize", w.DWORD), ("rcBar", w.RECT), ("hMenu", w.HMENU), ("hwndMenu", w.HWND), ("flags", w.DWORD)]


def foto(v, ruta):
    """De la barra de menús abajo, sin el marco ni el título: en un escritorio oculto no hay DWM y salen con el aspecto
    clásico de Windows."""
    texto = hijo(v, "RICHEDIT")
    if texto:
        u32.SendMessageW(texto, 0x8, 0, 0)   # WM_KILLFOCUS: sin el cursor parpadeando en la foto
    u32.RedrawWindow(v, None, None, 0x1 | 0x4 | 0x80 | 0x100 | 0x400)   # INVALIDATE | ERASE | ALLCHILDREN | UPDATENOW | FRAME
    time.sleep(0.6)
    ventana_r, cliente = w.RECT(), w.RECT()
    u32.GetWindowRect(v, ctypes.byref(ventana_r)); u32.GetClientRect(v, ctypes.byref(cliente))
    origen = w.POINT(0, 0); u32.ClientToScreen(v, ctypes.byref(origen))
    mb = MENUBARINFO(); mb.cbSize = ctypes.sizeof(MENUBARINFO)
    arriba = mb.rcBar.top if u32.GetMenuBarInfo(v, -3, 0, ctypes.byref(mb)) else origen.y   # OBJID_MENU
    x, y = origen.x - ventana_r.left, arriba - ventana_r.top
    an, al = cliente.right, origen.y + cliente.bottom - arriba
    dc = u32.GetWindowDC(v); m = g32.CreateCompatibleDC(dc); b = g32.CreateCompatibleBitmap(dc, an, al)
    viejo = g32.SelectObject(m, b)
    g32.BitBlt(m, 0, 0, an, al, dc, x, y, 0x00CC0020)
    bi = BIH(ctypes.sizeof(BIH), an, -al, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(an * al * 4)
    g32.GetDIBits(m, b, 0, al, buf, ctypes.byref(bi), 0)
    g32.SelectObject(m, viejo); g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(v, dc)
    Image.frombuffer("RGBX", (an, al), buf.raw, "raw", "BGRX", 0, 1).convert("RGB").save(ruta, optimize=True)
    print("hecha", os.path.basename(ruta), an, "x", al)


def con_bloc(escritorio, archivo, noche, hacer):
    with open(INI, "w") as f:
        f.write(f"ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche={noche}\ncorrector=0\nmarcas=0\nmargenes=1\n")
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO
    pi = PI()
    if not k32.CreateProcessW(None, ctypes.create_unicode_buffer(f'"{exe}" "{archivo}"'), None, None, False, 0, None,
                              None, ctypes.byref(si), ctypes.byref(pi)):
        sys.exit(f"CreateProcess: {k32.GetLastError()}")
    try:
        u32.WaitForInputIdle(pi.hProcess, 15000)
        v = None
        for _ in range(200):
            v = ventana(escritorio, pi.dwProcessId)
            if v and hijo(v, "RICHEDIT"):
                break
            time.sleep(0.05)
        if not v:
            sys.exit("No sale la ventana de Bloc")
        u32.MoveWindow(v, 0, 0, ANCHO, ALTO, True)
        time.sleep(0.8)
        hacer(v)
    finally:
        k32.TerminateProcess(pi.hProcess, 0)
        k32.CloseHandle(pi.hProcess); k32.CloseHandle(pi.hThread)


def trabajo():
    escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
    u32.SetThreadDesktop(escritorio)
    notas = os.path.join(dir, "notas.md")
    with open(notas, "w", encoding="utf-8", newline="\n") as f:
        f.write(NOTAS)

    def tamanos(noche):
        def hacer(v):
            u32.SendMessageW(v, WM_COMMAND, REEMPLAZAR, 0)
            time.sleep(0.5)
            for an, al in TAMANOS:
                u32.MoveWindow(v, 0, 0, an, al, True)
                time.sleep(0.4)
                foto(v, os.path.join(salida, f"{'oscuro' if noche else 'claro'}-{an}x{al}.png"))
        return hacer
    con_bloc(escritorio, notas, 0, tamanos(0))
    con_bloc(escritorio, notas, 1, tamanos(1))


hilo = threading.Thread(target=trabajo)
hilo.start(); hilo.join()
