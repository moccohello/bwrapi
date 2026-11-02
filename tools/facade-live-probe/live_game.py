"""live_game.py — Windows 실행 중인 게임에 키를 보내고 화면을 캡처하는 헬퍼.

사용법:
  python live_game.py keys --pid <pid> custom-game        # S E O U: 메인 메뉴 -> 커스텀 로비(여기서 shot)
  python live_game.py keys --pid <pid> custom-game-start  # O: 로비 -> 경기 시작
  python live_game.py keys --pid <pid> restart            # F10 E R R, 키 간 ~2초: 현재 경기 재시작
  python live_game.py keys --pid <pid> 83,69,79,85        # raw VK 코드. --delay <sec>로 키 간격 덮어쓰기
  python live_game.py shot --pid <pid> --out shot.png [--scale 0.5]
      PrintWindow(PW_RENDERFULLCONTENT) 캡처(가려진 창도 찍힘). PNG는 zlib 최소 인코더로 저장.
마우스 PostMessage는 동작하지 않는다(키만 지원).

게임은 사용자가 직접 실행한다(launch 하위 명령은 없다). Python 봇은 `python -m bwrapi.bwapi.run`으로 돌린다.
"""
from __future__ import annotations

import argparse
import ctypes
import json
import struct
import sys
import time
import zlib
from ctypes import byref, wintypes
from pathlib import Path

if sys.platform != "win32":
    sys.exit("live_game.py는 Windows 전용입니다")

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)

WM_KEYDOWN, WM_KEYUP = 0x0100, 0x0101
# 이름 -> (VK 목록, 키 간 대기 초)
SEQUENCES = {
    "custom-game": ([83, 69, 79, 85], 2.5),  # 메인 메뉴 -> 커스텀 게임 로비(화면 전환 대기)
    "custom-game-start": ([79], 0.4),        # 로비 -> 경기 시작
    "restart": ([121, 69, 82, 82], 2.0),     # F10 메뉴 -> Restart
}

# 64-bit 핸들은 기본 int 변환으로 넘기면 잘리므로 핸들을 주고받는 함수에 타입을 지정한다.
HGDI = wintypes.HGDIOBJ
user32.PostMessageW.argtypes = (wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM)
user32.GetDC.restype = wintypes.HDC
user32.GetDC.argtypes = (wintypes.HWND,)
user32.ReleaseDC.argtypes = (wintypes.HWND, wintypes.HDC)
user32.PrintWindow.argtypes = (wintypes.HWND, wintypes.HDC, wintypes.UINT)
gdi32.CreateCompatibleDC.restype = wintypes.HDC
gdi32.CreateCompatibleDC.argtypes = (wintypes.HDC,)
gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi32.CreateCompatibleBitmap.argtypes = (wintypes.HDC, ctypes.c_int, ctypes.c_int)
gdi32.SelectObject.restype = HGDI
gdi32.SelectObject.argtypes = (wintypes.HDC, HGDI)
gdi32.DeleteObject.argtypes = (HGDI,)
gdi32.DeleteDC.argtypes = (wintypes.HDC,)
gdi32.GetDIBits.argtypes = (wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT,
                            ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT)


def _fail(message: str) -> None:
    print(json.dumps({"error": message}, ensure_ascii=False), file=sys.stderr)
    raise SystemExit(1)


def _main_window(pid: int) -> int | None:
    windows = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def collect(hwnd, _lparam):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, byref(owner))
        # Owned windows of the process (dialogs) are not the game window.
        if owner.value == pid and user32.IsWindowVisible(hwnd) and not user32.GetWindow(hwnd, 4):  # GW_OWNER
            windows.append(hwnd)
        return True

    user32.EnumWindows(collect, 0)
    return windows[0] if windows else None


def cmd_keys(args: argparse.Namespace) -> None:
    if args.seq in SEQUENCES:
        codes, delay = SEQUENCES[args.seq]
    else:
        try:
            codes, delay = [int(vk) for vk in args.seq.split(",")], 0.4
        except ValueError:
            _fail(f"알 수 없는 시퀀스: {args.seq} (이름 또는 VK 코드 목록)")
    if args.delay is not None:
        delay = args.delay
    hwnd = _main_window(args.pid)
    if not hwnd:
        _fail(f"pid {args.pid}에 보이는 최상위 창이 없습니다")
    for index, vk in enumerate(codes):
        base = 1 | (user32.MapVirtualKeyW(vk, 0) << 16)  # repeat=1 | scan code
        user32.PostMessageW(hwnd, WM_KEYDOWN, vk, base)
        time.sleep(0.06)
        user32.PostMessageW(hwnd, WM_KEYUP, vk, base | (3 << 30))  # 이전 상태|전환 비트
        if index + 1 < len(codes):
            time.sleep(delay)
    print(json.dumps({"posted": codes, "pid": args.pid, "hwnd": hwnd, "delay": delay}))


class _BMH(ctypes.Structure):  # BITMAPINFOHEADER
    _fields_ = [
        ("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD),
    ]


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def _write_png(path: Path, width: int, height: int, rows: list[bytes]) -> None:
    raw = b"".join(b"\x00" + row for row in rows)  # 필터 0(Raw)
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)  # 8-bit RGB
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + _png_chunk(b"IHDR", header)
                     + _png_chunk(b"IDAT", zlib.compress(raw, 6)) + _png_chunk(b"IEND", b""))


def _downscale(raw: bytes, stride: int, width: int, height: int, out_w: int, out_h: int) -> list[bytes]:
    """DWORD 정렬 BGR top-down DIB를 박스 평균으로 축소해 RGB 행 리스트를 만든다."""
    col = [x * out_w // width for x in range(width)]
    col_count = [0] * out_w
    for dst in col:
        col_count[dst] += 1
    row_count = [0] * out_h
    for y in range(height):
        row_count[y * out_h // height] += 1
    rows, sums = [], None
    for y in range(height):
        dst_y = y * out_h // height
        if sums is None:
            sums = [[0, 0, 0] for _ in range(out_w)]
        offset = y * stride
        for x in range(width):
            pixel = sums[col[x]]
            pixel[0] += raw[offset]
            pixel[1] += raw[offset + 1]
            pixel[2] += raw[offset + 2]
            offset += 3
        if y + 1 == height or (y + 1) * out_h // height != dst_y:
            row = bytearray()
            for dst_x, total in enumerate(sums):
                count = col_count[dst_x] * row_count[dst_y]
                row += bytes((total[2] // count, total[1] // count, total[0] // count))
            rows.append(bytes(row))
            sums = None
    return rows


def cmd_shot(args: argparse.Namespace) -> None:
    if not 0 < args.scale <= 1:
        _fail("--scale은 0 초과 1 이하여야 합니다")
    user32.SetProcessDPIAware()
    hwnd = _main_window(args.pid)
    if not hwnd:
        _fail(f"pid {args.pid}에 보이는 최상위 창이 없습니다")
    rect = wintypes.RECT()
    user32.GetClientRect(hwnd, byref(rect))
    width, height = rect.right, rect.bottom
    if width <= 0 or height <= 0:
        _fail("클라이언트 영역 크기가 0입니다")
    hdc = user32.GetDC(hwnd)
    memory = gdi32.CreateCompatibleDC(hdc)
    bitmap = gdi32.CreateCompatibleBitmap(hdc, width, height)
    previous = gdi32.SelectObject(memory, bitmap)
    try:
        if not user32.PrintWindow(hwnd, memory, 3):  # PW_RENDERFULLCONTENT
            _fail("PrintWindow 실패")
        stride = (width * 3 + 3) & ~3
        pixels = ctypes.create_string_buffer(stride * height)
        info = _BMH(ctypes.sizeof(_BMH), width, -height, 1, 24)  # top-down 24bpp BGR
        if not gdi32.GetDIBits(hdc, bitmap, 0, height, pixels, byref(info), 0):
            _fail("GetDIBits 실패")
    finally:
        gdi32.SelectObject(memory, previous)
        gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(memory)
        user32.ReleaseDC(hwnd, hdc)
    out_w, out_h = max(1, int(width * args.scale)), max(1, int(height * args.scale))
    rows = _downscale(pixels.raw, stride, width, height, out_w, out_h)
    _write_png(Path(args.out), out_w, out_h, rows)
    print(json.dumps({"ok": True, "window": [width, height], "png": [out_w, out_h], "out": args.out}))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Windows 라이브 게임 헬퍼 (keys/shot)")
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("keys", help="PostMessage로 키 시퀀스 전송(포커스 변경 없음)")
    p.add_argument("--pid", type=int, required=True)
    p.add_argument("seq", help="custom-game | custom-game-start | restart | VK 코드(쉼표 구분)")
    p.add_argument("--delay", type=float, default=None, help="키 간 대기 초(기본: 시퀀스 기본값/raw 0.4)")
    p.set_defaults(func=cmd_keys)

    p = sub.add_parser("shot", help="PrintWindow 캡처를 PNG로 저장")
    p.add_argument("--pid", type=int, required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--scale", type=float, default=1.0)
    p.set_defaults(func=cmd_shot)

    args = parser.parse_args(argv)
    args.func(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
