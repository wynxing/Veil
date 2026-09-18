"""Veil 最小应用：托盘 + 物理屏列表 + VDD 辅助保持关闭。"""
from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBE = ROOT / "tools" / "display-probe"
sys.path.insert(0, str(PROBE))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import probe as p  # noqa: E402
import validation as v  # noqa: E402
from capability import (  # noqa: E402
    describe_hold_result,
    keep_off_block_reason,
    physical_paths,
    virtual_paths,
)

HOTKEY = v.HOTKEY
STATE_DIR = Path(os.environ.get("LOCALAPPDATA") or tempfile.gettempdir()) / "Veil"
MUTEX_NAME = "Local\\VeilMinApp"
ERROR_ALREADY_EXISTS = 183


def snapshot():
    return p.snapshot_status()


def check_payload(status=None):
    status = status or snapshot()
    return {
        "blockReason": keep_off_block_reason(status),
        "activeInternal": status.get("activeInternal"),
        "activeAuxiliary": status.get("activeAuxiliary"),
        "physical": [
            {
                "name": row.get("monitorName") or row.get("sourceName"),
                "role": row.get("role"),
                "active": row.get("active"),
            }
            for row in physical_paths(status)
        ],
        "virtualCount": len(virtual_paths(status)),
        "hotkey": HOTKEY,
        "sleepWake": "keep-off-ends-no-reapply",
    }


class Session:
    def __init__(self):
        self.directory = None
        self.proc = None
        self.config = None
        self.wanted = "开启"
        self.confirmed = "未知"
        self.detail = "尚未操作。"

    @property
    def holding(self):
        return self.proc is not None and self.proc.poll() is None

    def begin_keep_off(self):
        if self.holding:
            return
        v.check_layout()
        status = snapshot()
        blocked = keep_off_block_reason(status)
        if blocked:
            self.wanted = "保持关闭"
            self.confirmed = "失败"
            self.detail = blocked
            return
        prepared = v.validate_keep_off(adjust_clone=True)
        rc = prepared["rc"]
        count = prepared["disabledCount"]
        remaining = prepared["remainingActive"]
        if rc or count == 0 or remaining == 0:
            self.wanted = "保持关闭"
            self.confirmed = "失败"
            extra = "已尝试改为共用源拓扑。" if prepared["adjustedClone"] else ""
            self.detail = f"无法保持关闭：校验 {rc}，停用 {count}，剩余活动 {remaining}。{extra}"
            return
        note = "已改为共用源拓扑后校验通过。" if prepared["adjustedClone"] else ""
        STATE_DIR.mkdir(parents=True, exist_ok=True)
        directory = STATE_DIR / ("run-" + uuid.uuid4().hex[:8])
        config = directory / "topology.json"
        p.save_topology(str(config))
        self.wanted = "保持关闭"
        self.confirmed = "处理中"
        self.detail = f"独立恢复进程就绪后关屏。紧急恢复：{HOTKEY} {note}".strip()
        try:
            self.proc = v.start_run(
                str(config), directory, "disable-path", 0,
                parent_pid=os.getpid(), exist_ok=True,
            )
        except Exception as exc:
            self.confirmed = "失败"
            self.detail = f"未能启动恢复进程：{exc}"
            self.proc = None
            self.directory = None
            return
        self.directory = directory
        self.config = config

    def request_restore(self):
        self.wanted = "开启"
        if self.directory is None:
            status = snapshot()
            if keep_off_block_reason(status) is None:
                self.confirmed = "已确认"
                self.detail = "内屏已可用，无需恢复。"
                return
            rc = p.apply_topology_clone()
            self.confirmed = "已确认" if rc == 0 else "失败"
            self.detail = (
                "已尝试恢复内屏与虚拟目标共用源。" if rc == 0
                else f"恢复拓扑失败：{rc}"
            )
            return
        self.confirmed = "处理中"
        self.detail = "正在恢复关屏前拓扑。"
        v.request_release(self.directory)

    def poll(self):
        if self.directory is None:
            return
        result_path = self.directory / "result.json"
        if result_path.exists():
            result = v.read_json(result_path)
            self.proc = None
            self.wanted, self.confirmed, self.detail = describe_hold_result(
                result, snapshot(), HOTKEY
            )
            self.directory = None
            self.config = None
            return
        if self.proc is not None and self.proc.poll() is not None and not result_path.exists():
            rc = p.apply_topology_internal()
            self.wanted = "开启"
            self.confirmed = "失败"
            self.detail = f"恢复进程异常退出。点亮内屏返回 {rc}。"
            self.proc = None
            self.directory = None


def acquire_mutex():
    from ctypes import c_int32, c_uint32, c_void_p, c_wchar_p
    kernel32 = p.kernel32
    kernel32.CreateMutexW.argtypes = [c_void_p, c_int32, c_wchar_p]
    kernel32.CreateMutexW.restype = c_void_p
    kernel32.GetLastError.restype = c_uint32
    kernel32.SetLastError(0)
    handle = kernel32.CreateMutexW(None, True, MUTEX_NAME)
    if not handle:
        return None, False
    return handle, kernel32.GetLastError() != ERROR_ALREADY_EXISTS


def run_window(session: Session):
    import tkinter as tk

    root = tk.Tk()
    root.title("Veil")
    root.geometry("560x460")
    root.minsize(480, 400)

    wanted = tk.StringVar()
    confirmed = tk.StringVar()
    detail = tk.StringVar()
    listing = tk.StringVar()

    def render():
        status = snapshot()
        session.poll()
        lines = []
        for row in physical_paths(status):
            state = "活动" if row.get("active") else "未活动"
            kind = "内置" if row.get("role") == "internal" else "外接"
            name = row.get("monitorName") or row.get("sourceName") or "未命名"
            extra = ""
            if row.get("role") == "internal":
                blocked = keep_off_block_reason(status)
                extra = "  可保持关闭" if not blocked and not session.holding else ""
                if blocked:
                    extra = "  保持关闭不可用"
                if session.holding and row.get("active") is False:
                    extra = "  保持关闭中"
            elif row.get("role") == "external":
                extra = "  未验证"
            lines.append(f"{name}（{kind}，{state}）{extra}")
        virtual = virtual_paths(status)
        if virtual:
            lines.append(f"虚拟辅助目标 {len(virtual)} 个（不列入物理屏，不能对它关屏）")
        else:
            lines.append("未检测到活动虚拟目标")
        listing.set("\n".join(lines) or "没有显示器")
        wanted.set("用户要求：" + session.wanted)
        confirmed.set("已确认状态：" + session.confirmed)
        detail.set(session.detail)
        root.after(800, render)

    def on_keep_off():
        session.begin_keep_off()

    def on_restore():
        session.request_restore()

    def on_close():
        root.withdraw()

    def on_exit():
        if session.holding:
            session.request_restore()
            deadline = time.monotonic() + 20
            while session.holding and time.monotonic() < deadline:
                session.poll()
                root.update()
                time.sleep(0.2)
        root.destroy()

    tk.Label(root, text="Veil 最小应用", font=("Segoe UI", 16, "bold")).pack(anchor="w", padx=16, pady=(16, 4))
    tk.Label(
        root,
        text="仅覆盖本机已验证配置。虚拟屏不出现在可操作列表中。睡眠唤醒后不自动再关。",
        wraplength=520,
        justify="left",
    ).pack(anchor="w", padx=16)
    tk.Label(root, textvariable=listing, justify="left", font=("Segoe UI", 11)).pack(
        anchor="w", padx=16, pady=12
    )
    tk.Label(root, textvariable=wanted).pack(anchor="w", padx=16)
    tk.Label(root, textvariable=confirmed).pack(anchor="w", padx=16)
    tk.Label(root, textvariable=detail, wraplength=520, justify="left", fg="#444").pack(
        anchor="w", padx=16, pady=(4, 12)
    )
    buttons = tk.Frame(root)
    buttons.pack(anchor="w", padx=16, pady=8)
    tk.Button(buttons, text="保持关闭内屏", command=on_keep_off, width=16).pack(side="left", padx=(0, 8))
    tk.Button(buttons, text="恢复全部", command=on_restore, width=12).pack(side="left", padx=(0, 8))
    tk.Button(buttons, text="退出", command=on_exit, width=8).pack(side="left")
    tk.Label(root, text=f"紧急恢复：{HOTKEY}　关窗口会隐藏到托盘。", fg="#666").pack(
        anchor="w", padx=16, pady=(8, 0)
    )

    root.protocol("WM_DELETE_WINDOW", on_close)
    start_tray(root, session, on_restore, on_exit, lambda: (root.deiconify(), root.lift()))
    render()
    root.mainloop()


def start_tray(root, session, on_restore, on_exit, on_open):
    """托盘在后台线程处理菜单；动作切回 Tk 线程。"""

    def invoke(fn):
        root.after(0, fn)

    def worker():
        try:
            _tray_loop(invoke, on_open, on_restore, on_exit)
        except Exception:
            pass

    threading.Thread(target=worker, daemon=True).start()


def _tray_loop(invoke, on_open, on_restore, on_exit):
    import ctypes
    from ctypes import byref, sizeof, wintypes

    user32 = p.user32
    shell32 = ctypes.WinDLL("shell32", use_last_error=True)
    WM_APP = 0x8000
    WM_TRAY = WM_APP + 1
    WM_COMMAND = 0x0111
    WM_RBUTTONUP = 0x0205
    WM_LBUTTONDBLCLK = 0x0203
    NIM_ADD = 0
    NIM_DELETE = 2
    NIF_MESSAGE = 0x00000001
    NIF_ICON = 0x00000002
    NIF_TIP = 0x00000004
    IDI_APPLICATION = 32512
    ID_OPEN = 1
    ID_RESTORE = 2
    ID_EXIT = 3
    HWND_MESSAGE = -3

    class WNDCLASS(ctypes.Structure):
        _fields_ = [
            ("style", wintypes.UINT),
            ("lpfnWndProc", ctypes.c_void_p),
            ("cbClsExtra", ctypes.c_int),
            ("cbWndExtra", ctypes.c_int),
            ("hInstance", wintypes.HINSTANCE),
            ("hIcon", wintypes.HICON),
            ("hCursor", wintypes.HANDLE),
            ("hbrBackground", wintypes.HBRUSH),
            ("lpszMenuName", wintypes.LPCWSTR),
            ("lpszClassName", wintypes.LPCWSTR),
        ]

    class NOTIFYICONDATA(ctypes.Structure):
        _fields_ = [
            ("cbSize", wintypes.DWORD),
            ("hWnd", wintypes.HWND),
            ("uID", wintypes.UINT),
            ("uFlags", wintypes.UINT),
            ("uCallbackMessage", wintypes.UINT),
            ("hIcon", wintypes.HICON),
            ("szTip", wintypes.WCHAR * 128),
        ]

    WNDPROC = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, wintypes.HWND, wintypes.UINT, ctypes.c_size_t, ctypes.c_ssize_t
    )
    user32.DefWindowProcW.argtypes = [
        wintypes.HWND, wintypes.UINT, ctypes.c_size_t, ctypes.c_ssize_t
    ]
    user32.DefWindowProcW.restype = ctypes.c_ssize_t

    def show_menu(hwnd):
        menu = user32.CreatePopupMenu()
        user32.AppendMenuW(menu, 0, ID_OPEN, "打开 Veil")
        user32.AppendMenuW(menu, 0, ID_RESTORE, "恢复全部屏幕")
        user32.AppendMenuW(menu, 0, ID_EXIT, "退出")
        pt = wintypes.POINT()
        user32.GetCursorPos(byref(pt))
        user32.SetForegroundWindow(hwnd)
        user32.TrackPopupMenu(menu, 0, pt.x, pt.y, 0, hwnd, None)
        user32.DestroyMenu(menu)

    def wnd_proc(hwnd, msg, wparam, lparam):
        if msg == WM_TRAY:
            if lparam == WM_RBUTTONUP:
                show_menu(hwnd)
            elif lparam == WM_LBUTTONDBLCLK:
                invoke(on_open)
        elif msg == WM_COMMAND:
            command = wparam & 0xFFFF
            if command == ID_OPEN:
                invoke(on_open)
            elif command == ID_RESTORE:
                invoke(on_restore)
            elif command == ID_EXIT:
                invoke(on_exit)
        return user32.DefWindowProcW(hwnd, msg, wparam, lparam)

    proc = WNDPROC(wnd_proc)
    globals()["_veil_tray_proc"] = proc
    class_name = "VeilTrayClass"
    wc = WNDCLASS()
    wc.lpfnWndProc = ctypes.cast(proc, ctypes.c_void_p).value
    wc.hInstance = kernel_instance()
    wc.lpszClassName = class_name
    if not user32.RegisterClassW(byref(wc)):
        return
    hwnd = user32.CreateWindowExW(
        0, class_name, "VeilTray", 0, 0, 0, 0, 0, ctypes.c_void_p(HWND_MESSAGE), None, wc.hInstance, None
    )
    if not hwnd:
        return
    nid = NOTIFYICONDATA()
    nid.cbSize = sizeof(NOTIFYICONDATA)
    nid.hWnd = hwnd
    nid.uID = 1
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP
    nid.uCallbackMessage = WM_TRAY
    nid.hIcon = user32.LoadIconW(None, ctypes.cast(IDI_APPLICATION, ctypes.c_wchar_p))
    nid.szTip = "Veil"
    shell32.Shell_NotifyIconW(NIM_ADD, byref(nid))
    msg = wintypes.MSG()
    while user32.GetMessageW(byref(msg), None, 0, 0):
        user32.TranslateMessage(byref(msg))
        user32.DispatchMessageW(byref(msg))
    shell32.Shell_NotifyIconW(NIM_DELETE, byref(nid))


def kernel_instance():
    return p.kernel32.GetModuleHandleW(None)


def main(argv=None):
    parser = argparse.ArgumentParser(description="Veil 最小应用")
    parser.add_argument("--check", action="store_true", help="只打印门禁，不关屏")
    args = parser.parse_args(argv)
    if args.check:
        payload = check_payload()
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return 0 if payload["blockReason"] is None else 2
    handle, owned = acquire_mutex()
    if not owned:
        print("Veil 已在运行。")
        return 2
    try:
        run_window(Session())
        return 0
    finally:
        if handle:
            p.kernel32.CloseHandle(handle)


if __name__ == "__main__":
    sys.exit(main())
