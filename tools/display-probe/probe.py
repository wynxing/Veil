"""Windows display-control probe for Veil technical validation.

Not a product application. Python 3.12 standard library only.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import subprocess
import sys
import time
from ctypes import (
    POINTER,
    Structure,
    Union,
    WinDLL,
    byref,
    c_int32,
    c_int64,
    c_long,
    c_uint16,
    c_uint32,
    c_uint64,
    c_void_p,
    c_wchar,
    create_unicode_buffer,
    get_last_error,
    sizeof,
)
from datetime import datetime, timezone
from typing import Any

ERROR_SUCCESS = 0
ERROR_INSUFFICIENT_BUFFER = 122

QDC_ONLY_ACTIVE_PATHS = 0x00000002
QDC_VIRTUAL_MODE_AWARE = 0x00000010
QDC_VIRTUAL_REFRESH_RATE_AWARE = 0x00000040

SDC_TOPOLOGY_INTERNAL = 0x00000001
SDC_USE_SUPPLIED_DISPLAY_CONFIG = 0x00000020
SDC_VALIDATE = 0x00000040
SDC_APPLY = 0x00000080
SDC_SAVE_TO_DATABASE = 0x00000200
SDC_ALLOW_CHANGES = 0x00000400
SDC_VIRTUAL_MODE_AWARE = 0x00008000
SDC_VIRTUAL_REFRESH_RATE_AWARE = 0x00020000

DISPLAYCONFIG_PATH_ACTIVE = 0x00000001

DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME = 1
DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME = 2
DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME = 4

DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL = 0x80000000
DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED = 11
DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED = 13

HWND_BROADCAST = 0xFFFF
WM_SYSCOMMAND = 0x0112
SC_MONITORPOWER = 0xF170
SMTO_ABORTIFHUNG = 0x0002
SM_CMONITORS = 80

CREATE_NEW_PROCESS_GROUP = 0x00000200
CREATE_NO_WINDOW = 0x08000000
CREATE_BREAKAWAY_FROM_JOB = 0x01000000
MOUSEEVENTF_MOVE = 0x0001
KEYEVENTF_KEYUP = 0x0002
VK_SPACE = 0x20

QUERY_FLAGS = (
    QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE
)
SET_BASE_FLAGS = (
    SDC_USE_SUPPLIED_DISPLAY_CONFIG
    | SDC_ALLOW_CHANGES
    | SDC_VIRTUAL_MODE_AWARE
    | SDC_VIRTUAL_REFRESH_RATE_AWARE
)

user32 = WinDLL("user32", use_last_error=True)
kernel32 = WinDLL("kernel32", use_last_error=True)


class LUID(Structure):
    _fields_ = [("LowPart", c_uint32), ("HighPart", c_int32)]


class DISPLAYCONFIG_RATIONAL(Structure):
    _fields_ = [("Numerator", c_uint32), ("Denominator", c_uint32)]


class DISPLAYCONFIG_2DREGION(Structure):
    _fields_ = [("cx", c_uint32), ("cy", c_uint32)]


class DISPLAYCONFIG_PATH_SOURCE_INFO(Structure):
    _fields_ = [
        ("adapterId", LUID),
        ("id", c_uint32),
        ("modeInfoIdx", c_uint32),
        ("statusFlags", c_uint32),
    ]


class DISPLAYCONFIG_PATH_TARGET_INFO(Structure):
    _fields_ = [
        ("adapterId", LUID),
        ("id", c_uint32),
        ("modeInfoIdx", c_uint32),
        ("outputTechnology", c_uint32),
        ("rotation", c_uint32),
        ("scaling", c_uint32),
        ("refreshRate", DISPLAYCONFIG_RATIONAL),
        ("scanLineOrdering", c_uint32),
        ("targetAvailable", c_int32),
        ("statusFlags", c_uint32),
    ]


class DISPLAYCONFIG_PATH_INFO(Structure):
    _fields_ = [
        ("sourceInfo", DISPLAYCONFIG_PATH_SOURCE_INFO),
        ("targetInfo", DISPLAYCONFIG_PATH_TARGET_INFO),
        ("flags", c_uint32),
    ]


class DISPLAYCONFIG_VIDEO_SIGNAL_INFO(Structure):
    _fields_ = [
        ("pixelRate", c_uint64),
        ("hSyncFreq", DISPLAYCONFIG_RATIONAL),
        ("vSyncFreq", DISPLAYCONFIG_RATIONAL),
        ("activeSize", DISPLAYCONFIG_2DREGION),
        ("totalSize", DISPLAYCONFIG_2DREGION),
        ("videoStandard", c_uint32),
        ("scanLineOrdering", c_uint32),
    ]


class DISPLAYCONFIG_TARGET_MODE(Structure):
    _fields_ = [("targetVideoSignalInfo", DISPLAYCONFIG_VIDEO_SIGNAL_INFO)]


class POINTL(Structure):
    _fields_ = [("x", c_int32), ("y", c_int32)]


class DISPLAYCONFIG_SOURCE_MODE(Structure):
    _fields_ = [
        ("width", c_uint32),
        ("height", c_uint32),
        ("pixelFormat", c_uint32),
        ("position", POINTL),
    ]


class RECT(Structure):
    _fields_ = [
        ("left", c_int32),
        ("top", c_int32),
        ("right", c_int32),
        ("bottom", c_int32),
    ]


class DISPLAYCONFIG_DESKTOP_IMAGE_INFO(Structure):
    _fields_ = [
        ("PathSourceSize", POINTL),
        ("DesktopImageRegion", RECT),
        ("DesktopImageClip", RECT),
    ]


class DISPLAYCONFIG_MODE_INFO_UNION(Union):
    _fields_ = [
        ("targetMode", DISPLAYCONFIG_TARGET_MODE),
        ("sourceMode", DISPLAYCONFIG_SOURCE_MODE),
        ("desktopImageInfo", DISPLAYCONFIG_DESKTOP_IMAGE_INFO),
    ]


class DISPLAYCONFIG_MODE_INFO(Structure):
    _anonymous_ = ("mode",)
    _fields_ = [
        ("infoType", c_uint32),
        ("id", c_uint32),
        ("adapterId", LUID),
        ("mode", DISPLAYCONFIG_MODE_INFO_UNION),
    ]


class DISPLAYCONFIG_DEVICE_INFO_HEADER(Structure):
    _fields_ = [
        ("type", c_uint32),
        ("size", c_uint32),
        ("adapterId", LUID),
        ("id", c_uint32),
    ]


class DISPLAYCONFIG_TARGET_DEVICE_NAME(Structure):
    _fields_ = [
        ("header", DISPLAYCONFIG_DEVICE_INFO_HEADER),
        ("flags", c_uint32),
        ("outputTechnology", c_uint32),
        ("edidManufactureId", c_uint16),
        ("edidProductCodeId", c_uint16),
        ("connectorInstance", c_uint32),
        ("monitorFriendlyDeviceName", c_wchar * 64),
        ("monitorDevicePath", c_wchar * 128),
    ]


class DISPLAYCONFIG_ADAPTER_NAME(Structure):
    _fields_ = [
        ("header", DISPLAYCONFIG_DEVICE_INFO_HEADER),
        ("adapterDevicePath", c_wchar * 128),
    ]


class DISPLAYCONFIG_SOURCE_DEVICE_NAME(Structure):
    _fields_ = [
        ("header", DISPLAYCONFIG_DEVICE_INFO_HEADER),
        ("viewGdiDeviceName", c_wchar * 32),
    ]


class DISPLAY_DEVICEW(Structure):
    _fields_ = [
        ("cb", c_uint32),
        ("DeviceName", c_wchar * 32),
        ("DeviceString", c_wchar * 128),
        ("StateFlags", c_uint32),
        ("DeviceID", c_wchar * 128),
        ("DeviceKey", c_wchar * 128),
    ]


user32.GetDisplayConfigBufferSizes.argtypes = [c_uint32, POINTER(c_uint32), POINTER(c_uint32)]
user32.GetDisplayConfigBufferSizes.restype = c_long
user32.QueryDisplayConfig.argtypes = [
    c_uint32,
    POINTER(c_uint32),
    c_void_p,
    POINTER(c_uint32),
    c_void_p,
    POINTER(c_uint32),
]
user32.QueryDisplayConfig.restype = c_long
user32.SetDisplayConfig.argtypes = [c_uint32, c_void_p, c_uint32, c_void_p, c_uint32]
user32.SetDisplayConfig.restype = c_long
user32.DisplayConfigGetDeviceInfo.argtypes = [c_void_p]
user32.DisplayConfigGetDeviceInfo.restype = c_long
user32.SendNotifyMessageW.argtypes = [c_void_p, c_uint32, c_uint64, c_int64]
user32.SendNotifyMessageW.restype = c_int32
kernel32.FormatMessageW.argtypes = [
    c_uint32,
    c_void_p,
    c_uint32,
    c_uint32,
    c_void_p,
    c_uint32,
    c_void_p,
]
kernel32.FormatMessageW.restype = c_uint32
user32.GetSystemMetrics.argtypes = [c_int32]
user32.GetSystemMetrics.restype = c_int32
user32.EnumDisplayDevicesW.argtypes = [c_void_p, c_uint32, POINTER(DISPLAY_DEVICEW), c_uint32]
user32.EnumDisplayDevicesW.restype = c_int32
user32.mouse_event.argtypes = [c_uint32, c_uint32, c_uint32, c_uint32, c_uint64]
user32.mouse_event.restype = None
user32.keybd_event.argtypes = [c_uint32, c_uint32, c_uint32, c_uint64]
user32.keybd_event.restype = None


def win_message(code: int) -> str:
    buf = create_unicode_buffer(1024)
    flags = 0x00001000 | 0x00000200
    n = kernel32.FormatMessageW(flags, None, code & 0xFFFFFFFF, 0, buf, 1024, None)
    text = buf.value.strip() if n else ""
    return f"{code} {text}".strip()


def clocks() -> dict[str, Any]:
    return {
        "wallUnix": time.time(),
        "monotonic": time.monotonic(),
        "wall": datetime.now(timezone.utc).isoformat(),
    }


def log_event(log_path: str | None, event: dict[str, Any]) -> None:
    payload = {"ts": datetime.now(timezone.utc).isoformat(), "clocks": clocks(), **event}
    line = json.dumps(payload, ensure_ascii=False)
    print(line, flush=True)
    if log_path:
        os.makedirs(os.path.dirname(os.path.abspath(log_path)) or ".", exist_ok=True)
        with open(log_path, "a", encoding="utf-8") as handle:
            handle.write(line + "\n")
            handle.flush()


def luid_str(value: LUID) -> str:
    return f"{value.HighPart:08x}{value.LowPart:08x}"


def is_internal_tech(tech: int) -> bool:
    return tech in {
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
    }


def looks_virtual(row: dict[str, Any]) -> bool:
    blob = " ".join(
        str(row.get(key) or "")
        for key in ("adapterPath", "monitorPath", "monitorName", "sourceName")
    ).lower()
    needles = (
        "root\\display",
        "root#display",
        "iddcx",
        "virtual",
        "usbmmidd",
        "virtualdisplay",
        "indirect",
        "idd sample",
        "mtt",
    )
    return any(needle in blob for needle in needles)


def classify_role(row: dict[str, Any]) -> str:
    if row.get("placeholder"):
        return "placeholder"
    if row.get("internal"):
        return "internal"
    if looks_virtual(row):
        return "virtual"
    return "external"


def query_topology(flags: int = QUERY_FLAGS) -> tuple[Any, Any]:
    path_count = c_uint32()
    mode_count = c_uint32()
    for _ in range(8):
        rc = user32.GetDisplayConfigBufferSizes(flags, byref(path_count), byref(mode_count))
        if rc != ERROR_SUCCESS:
            raise OSError(rc, f"GetDisplayConfigBufferSizes failed: {win_message(rc)}")
        paths = (DISPLAYCONFIG_PATH_INFO * path_count.value)()
        modes = (DISPLAYCONFIG_MODE_INFO * mode_count.value)()
        rc = user32.QueryDisplayConfig(
            flags,
            byref(path_count),
            paths,
            byref(mode_count),
            modes,
            None,
        )
        if rc == ERROR_INSUFFICIENT_BUFFER:
            continue
        if rc != ERROR_SUCCESS:
            raise OSError(rc, f"QueryDisplayConfig failed: {win_message(rc)}")
        return paths[: path_count.value], modes[: mode_count.value]
    raise OSError(ERROR_INSUFFICIENT_BUFFER, "QueryDisplayConfig buffer retry exhausted")


def set_display_config(paths: list[Any], modes: list[Any], flags: int) -> int:
    path_array = (DISPLAYCONFIG_PATH_INFO * len(paths))(*paths)
    mode_array = (DISPLAYCONFIG_MODE_INFO * len(modes))(*modes) if modes else None
    return int(
        user32.SetDisplayConfig(
            len(paths),
            path_array,
            len(modes),
            mode_array,
            flags,
        )
    )


def device_target_name(path: DISPLAYCONFIG_PATH_INFO) -> dict[str, Any]:
    info = DISPLAYCONFIG_TARGET_DEVICE_NAME()
    info.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME
    info.header.size = sizeof(info)
    info.header.adapterId = path.targetInfo.adapterId
    info.header.id = path.targetInfo.id
    rc = user32.DisplayConfigGetDeviceInfo(byref(info))
    return {
        "rc": rc,
        "name": info.monitorFriendlyDeviceName if rc == 0 else "",
        "path": info.monitorDevicePath if rc == 0 else "",
        "outputTechnology": int(info.outputTechnology) if rc == 0 else None,
        "edidManufactureId": int(info.edidManufactureId) if rc == 0 else None,
        "edidProductCodeId": int(info.edidProductCodeId) if rc == 0 else None,
    }


def device_adapter_name(path: DISPLAYCONFIG_PATH_INFO) -> str:
    info = DISPLAYCONFIG_ADAPTER_NAME()
    info.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME
    info.header.size = sizeof(info)
    info.header.adapterId = path.targetInfo.adapterId
    info.header.id = path.targetInfo.id
    rc = user32.DisplayConfigGetDeviceInfo(byref(info))
    return info.adapterDevicePath if rc == 0 else ""


def device_source_name(path: DISPLAYCONFIG_PATH_INFO) -> str:
    info = DISPLAYCONFIG_SOURCE_DEVICE_NAME()
    info.header.type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME
    info.header.size = sizeof(info)
    info.header.adapterId = path.sourceInfo.adapterId
    info.header.id = path.sourceInfo.id
    rc = user32.DisplayConfigGetDeviceInfo(byref(info))
    return info.viewGdiDeviceName if rc == 0 else ""


def enum_gdi_devices() -> list[dict[str, Any]]:
    devices: list[dict[str, Any]] = []
    adapter_index = 0
    while True:
        adapter = DISPLAY_DEVICEW()
        adapter.cb = sizeof(adapter)
        if not user32.EnumDisplayDevicesW(None, adapter_index, byref(adapter), 0):
            break
        monitors: list[dict[str, Any]] = []
        monitor_index = 0
        while True:
            monitor = DISPLAY_DEVICEW()
            monitor.cb = sizeof(monitor)
            if not user32.EnumDisplayDevicesW(adapter.DeviceName, monitor_index, byref(monitor), 0):
                break
            monitors.append(
                {
                    "name": monitor.DeviceName,
                    "string": monitor.DeviceString,
                    "id": monitor.DeviceID,
                    "stateFlags": int(monitor.StateFlags),
                    "placeholder": "DEFAULT_MONITOR" in monitor.DeviceID,
                }
            )
            monitor_index += 1
        devices.append(
            {
                "name": adapter.DeviceName,
                "string": adapter.DeviceString,
                "id": adapter.DeviceID,
                "stateFlags": int(adapter.StateFlags),
                "monitors": monitors,
            }
        )
        adapter_index += 1
    return devices


def snapshot_status() -> dict[str, Any]:
    paths, modes = query_topology()
    path_rows = []
    for index, path in enumerate(paths):
        target = device_target_name(path)
        device_path = target.get("path") or ""
        row = {
            "index": index,
            "active": bool(path.flags & DISPLAYCONFIG_PATH_ACTIVE),
            "flags": int(path.flags),
            "sourceId": int(path.sourceInfo.id),
            "targetId": int(path.targetInfo.id),
            "adapterId": luid_str(path.targetInfo.adapterId),
            "outputTechnology": int(path.targetInfo.outputTechnology),
            "internal": is_internal_tech(int(path.targetInfo.outputTechnology)),
            "targetAvailable": bool(path.targetInfo.targetAvailable),
            "sourceName": device_source_name(path),
            "adapterPath": device_adapter_name(path),
            "monitorName": target.get("name"),
            "monitorPath": device_path,
            "placeholder": "DEFAULT_MONITOR" in device_path,
            "edidManufactureId": target.get("edidManufactureId"),
            "edidProductCodeId": target.get("edidProductCodeId"),
        }
        row["role"] = classify_role(row)
        path_rows.append(row)
    roles = [row["role"] for row in path_rows if row["active"]]
    return {
        "pathCount": len(paths),
        "modeCount": len(modes),
        "gdiMonitorCount": int(user32.GetSystemMetrics(SM_CMONITORS)),
        "pathInfoSize": sizeof(DISPLAYCONFIG_PATH_INFO),
        "modeInfoSize": sizeof(DISPLAYCONFIG_MODE_INFO),
        "activeInternal": roles.count("internal"),
        "activeAuxiliary": roles.count("virtual") + roles.count("external"),
        "paths": path_rows,
        "gdi": enum_gdi_devices(),
    }


def topology_to_dict(paths: list[Any], modes: list[Any]) -> dict[str, Any]:
    path_array = (DISPLAYCONFIG_PATH_INFO * len(paths))(*paths)
    mode_array = (DISPLAYCONFIG_MODE_INFO * len(modes))(*modes)
    return {
        "version": 1,
        "queryFlags": QUERY_FLAGS,
        "pathCount": len(paths),
        "modeCount": len(modes),
        "pathB64": base64.b64encode(bytes(path_array)).decode("ascii"),
        "modeB64": base64.b64encode(bytes(mode_array)).decode("ascii"),
        "savedAt": datetime.now(timezone.utc).isoformat(),
    }


def topology_from_dict(data: dict[str, Any]) -> tuple[list[Any], list[Any]]:
    path_count = int(data["pathCount"])
    mode_count = int(data["modeCount"])
    path_raw = base64.b64decode(data["pathB64"])
    mode_raw = base64.b64decode(data["modeB64"])
    expected_path = sizeof(DISPLAYCONFIG_PATH_INFO) * path_count
    expected_mode = sizeof(DISPLAYCONFIG_MODE_INFO) * mode_count
    if len(path_raw) != expected_path or len(mode_raw) != expected_mode:
        raise ValueError(
            f"topology size mismatch: paths {len(path_raw)}!={expected_path}, "
            f"modes {len(mode_raw)}!={expected_mode}"
        )
    path_array = (DISPLAYCONFIG_PATH_INFO * path_count).from_buffer_copy(path_raw)
    mode_array = (DISPLAYCONFIG_MODE_INFO * mode_count).from_buffer_copy(mode_raw)
    return list(path_array), list(mode_array)


def save_topology(path: str) -> dict[str, Any]:
    paths, modes = query_topology()
    data = topology_to_dict(paths, modes)
    os.makedirs(os.path.dirname(os.path.abspath(path)) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2)
        handle.write("\n")
    return data


def load_topology(path: str) -> tuple[list[Any], list[Any]]:
    with open(path, encoding="utf-8") as handle:
        return topology_from_dict(json.load(handle))


def restore_topology(path: str) -> int:
    paths, modes = load_topology(path)
    return set_display_config(
        paths,
        modes,
        SET_BASE_FLAGS | SDC_APPLY | SDC_SAVE_TO_DATABASE,
    )


def apply_topology_internal() -> int:
    return int(
        user32.SetDisplayConfig(
            0,
            None,
            0,
            None,
            SDC_APPLY | SDC_TOPOLOGY_INTERNAL | SDC_VIRTUAL_MODE_AWARE,
        )
    )


def monitor_power(state: int) -> tuple[int, int]:
    # HWND_BROADCAST + SendMessageTimeout can block for tens of seconds.
    sent = user32.SendNotifyMessageW(
        HWND_BROADCAST,
        WM_SYSCOMMAND,
        SC_MONITORPOWER,
        state,
    )
    last_error = 0 if sent else get_last_error()
    return int(sent), last_error


def synthesize_input() -> None:
    user32.mouse_event(MOUSEEVENTF_MOVE, 12, 0, 0, 0)
    time.sleep(0.05)
    user32.keybd_event(VK_SPACE, 0, 0, 0)
    user32.keybd_event(VK_SPACE, 0, KEYEVENTF_KEYUP, 0)


def start_watchdog(config: str, seconds: int, log_path: str) -> int:
    script = os.path.abspath(__file__)
    command = [
        sys.executable,
        script,
        "watchdog",
        "--config",
        os.path.abspath(config),
        "--seconds",
        str(seconds),
        "--log",
        os.path.abspath(log_path),
    ]
    flags = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB
    try:
        proc = subprocess.Popen(
            command,
            creationflags=flags,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
        )
    except OSError:
        proc = subprocess.Popen(
            command,
            creationflags=CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
        )
    return int(proc.pid)


def require_confirm(value: str) -> None:
    if value != "off":
        raise SystemExit("refusing to run: pass --confirm off")


def wait_and_poll(log_path: str, seconds: int, note: str) -> None:
    start_wall = time.time()
    start_mono = time.monotonic()
    deadline_mono = start_mono + seconds
    while True:
        now_mono = time.monotonic()
        remaining = max(0.0, deadline_mono - now_mono)
        wall_elapsed = time.time() - start_wall
        mono_elapsed = now_mono - start_mono
        status = snapshot_status()
        log_event(
            log_path,
            {
                "event": "poll",
                "note": note,
                "remainingSeconds": round(remaining, 1),
                "wallElapsedSeconds": round(wall_elapsed, 1),
                "monoElapsedSeconds": round(mono_elapsed, 1),
                "sleepSuspected": wall_elapsed > mono_elapsed + 2.0,
                "gdiMonitorCount": status["gdiMonitorCount"],
                "activeInternal": status["activeInternal"],
                "activeAuxiliary": status["activeAuxiliary"],
                "activePaths": [row for row in status["paths"] if row["active"]],
            },
        )
        if remaining <= 0:
            break
        time.sleep(min(5.0, remaining))


def cmd_enumerate(_args: argparse.Namespace) -> int:
    status = snapshot_status()
    print(json.dumps(status, ensure_ascii=False, indent=2))
    return 0


def cmd_save(args: argparse.Namespace) -> int:
    data = save_topology(args.config)
    log_event(
        args.log,
        {
            "event": "save",
            "config": os.path.abspath(args.config),
            "pathCount": data["pathCount"],
            "modeCount": data["modeCount"],
        },
    )
    return 0


def cmd_restore(args: argparse.Namespace) -> int:
    rc = restore_topology(args.config)
    log_event(
        args.log,
        {
            "event": "restore",
            "config": os.path.abspath(args.config),
            "rc": rc,
            "message": win_message(rc) if rc else "ERROR_SUCCESS",
        },
    )
    return 0 if rc == 0 else 1


def cmd_watchdog(args: argparse.Namespace) -> int:
    log_event(
        args.log,
        {
            "event": "watchdog_sleep",
            "pid": os.getpid(),
            "seconds": args.seconds,
            "config": os.path.abspath(args.config),
        },
    )
    time.sleep(args.seconds)
    rc = restore_topology(args.config)
    log_event(
        args.log,
        {
            "event": "watchdog_restore",
            "rc": rc,
            "message": win_message(rc) if rc else "ERROR_SUCCESS",
        },
    )
    sent, err = monitor_power(-1)
    log_event(
        args.log,
        {
            "event": "watchdog_monitor_on",
            "sent": sent,
            "lastError": err,
            "lastErrorMessage": win_message(err) if err else "",
        },
    )
    if rc != 0:
        fallback = apply_topology_internal()
        log_event(
            args.log,
            {
                "event": "watchdog_topology_internal",
                "rc": fallback,
                "message": win_message(fallback) if fallback else "ERROR_SUCCESS",
            },
        )
        return 0 if fallback == 0 else 1
    return 0


def cmd_temp_off(args: argparse.Namespace) -> int:
    require_confirm(args.confirm)
    save_topology(args.config)
    pid = start_watchdog(args.config, args.watchdog_seconds, args.log)
    log_event(
        args.log,
        {
            "event": "watchdog_started",
            "pid": pid,
            "seconds": args.watchdog_seconds,
            "recoveryHint": "Win+Ctrl+Shift+B then reboot if restore fails",
        },
    )
    before = snapshot_status()
    log_event(args.log, {"event": "status_before", "status": before})
    sent, err = monitor_power(2)
    log_event(
        args.log,
        {
            "event": "temp_off",
            "sent": sent,
            "lastError": err,
            "lastErrorMessage": win_message(err) if err else "",
        },
    )
    wait_and_poll(args.log, args.watchdog_seconds + 3, "temp-off-wait-restore")
    after = snapshot_status()
    log_event(args.log, {"event": "status_after", "status": after})
    return 0 if sent else 1


def deactivated_paths(paths: list[Any], target: str) -> tuple[list[Any], int, int]:
    changed = []
    disabled = 0
    remaining_active = 0
    for path in paths:
        clone = DISPLAYCONFIG_PATH_INFO.from_buffer_copy(bytes(path))
        is_internal = is_internal_tech(int(clone.targetInfo.outputTechnology))
        should_disable = clone.flags & DISPLAYCONFIG_PATH_ACTIVE and (
            target == "all" or (target == "internal" and is_internal)
        )
        if should_disable:
            clone.flags &= ~DISPLAYCONFIG_PATH_ACTIVE
            disabled += 1
        elif clone.flags & DISPLAYCONFIG_PATH_ACTIVE:
            remaining_active += 1
        changed.append(clone)
    return changed, disabled, remaining_active


def cmd_disable_path(args: argparse.Namespace) -> int:
    require_confirm(args.confirm)
    save_topology(args.config)
    paths, modes = load_topology(args.config)
    disabled, disabled_count, remaining_active = deactivated_paths(paths, args.target)
    log_event(
        args.log,
        {
            "event": "disable_path_plan",
            "target": args.target,
            "disabledCount": disabled_count,
            "remainingActive": remaining_active,
            "validateOnly": bool(args.validate_only),
        },
    )
    if args.target == "internal" and remaining_active < 1:
        log_event(
            args.log,
            {
                "event": "disable_path_skipped_apply",
                "reason": "no remaining active path after disabling internal; auxiliary target required",
            },
        )
        return 3

    validate_rc = set_display_config(disabled, modes, SET_BASE_FLAGS | SDC_VALIDATE)
    log_event(
        args.log,
        {
            "event": "disable_path_validate",
            "rc": validate_rc,
            "message": win_message(validate_rc) if validate_rc else "ERROR_SUCCESS",
            "target": args.target,
            "disabledCount": disabled_count,
            "remainingActive": remaining_active,
        },
    )
    if validate_rc != 0:
        log_event(
            args.log,
            {
                "event": "disable_path_skipped_apply",
                "reason": "validate failed; path disable not applied",
            },
        )
        return 2
    if args.validate_only:
        log_event(args.log, {"event": "disable_path_validate_only_done"})
        return 0

    pid = start_watchdog(args.config, args.watchdog_seconds, args.log)
    log_event(
        args.log,
        {
            "event": "watchdog_started",
            "pid": pid,
            "seconds": args.watchdog_seconds,
            "recoveryHint": "Win+Ctrl+Shift+B then reboot if restore fails",
        },
    )
    apply_rc = set_display_config(
        disabled,
        modes,
        SET_BASE_FLAGS | SDC_APPLY | SDC_SAVE_TO_DATABASE,
    )
    log_event(
        args.log,
        {
            "event": "disable_path_apply",
            "rc": apply_rc,
            "message": win_message(apply_rc) if apply_rc else "ERROR_SUCCESS",
        },
    )
    if args.input_test:
        time.sleep(3)
        synthesize_input()
        log_event(args.log, {"event": "input_synthesized"})
        time.sleep(2)
        log_event(args.log, {"event": "status_after_input", "status": snapshot_status()})
    wait_and_poll(args.log, args.watchdog_seconds + 3, "disable-path-wait-restore")
    log_event(args.log, {"event": "status_after", "status": snapshot_status()})
    return 0 if apply_rc == 0 else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Veil display-control validation probe")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("enumerate", help="list CCD paths and GDI devices")

    save = sub.add_parser("save", help="save current topology bytes")
    save.add_argument("--config", required=True)
    save.add_argument("--log")

    restore = sub.add_parser("restore", help="apply saved topology")
    restore.add_argument("--config", required=True)
    restore.add_argument("--log")

    watchdog = sub.add_parser("watchdog", help="sleep then restore saved topology")
    watchdog.add_argument("--config", required=True)
    watchdog.add_argument("--seconds", type=int, required=True)
    watchdog.add_argument("--log", required=True)

    temp_off = sub.add_parser("temp-off", help="SC_MONITORPOWER off with watchdog")
    temp_off.add_argument("--config", required=True)
    temp_off.add_argument("--log", required=True)
    temp_off.add_argument("--watchdog-seconds", type=int, default=20)
    temp_off.add_argument("--confirm", required=True)

    disable = sub.add_parser("disable-path", help="CCD deactivate paths with watchdog")
    disable.add_argument("--config", required=True)
    disable.add_argument("--log", required=True)
    disable.add_argument("--watchdog-seconds", type=int, default=15)
    disable.add_argument("--confirm", required=True)
    disable.add_argument("--target", choices=("internal", "all"), default="internal")
    disable.add_argument("--validate-only", action="store_true")
    disable.add_argument("--input-test", action="store_true")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    commands = {
        "enumerate": cmd_enumerate,
        "save": cmd_save,
        "restore": cmd_restore,
        "watchdog": cmd_watchdog,
        "temp-off": cmd_temp_off,
        "disable-path": cmd_disable_path,
    }
    try:
        return commands[args.command](args)
    except OSError as exc:
        log_event(getattr(args, "log", None), {"event": "os_error", "message": str(exc)})
        return 1


if __name__ == "__main__":
    sys.exit(main())
