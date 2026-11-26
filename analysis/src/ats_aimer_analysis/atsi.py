"""Lecteur du format binaire `input.atsi` (ATS-AIMER recorder).

Layout :
    header 32B : "ATSI" | u16 version | u16 hdr_size | u64 qpc_freq
                 | u64 qpc_start | u64 unix_start_ms
    record 24B : u64 qpc | u16 kind | u16 flags | i32 a | i32 b | i32 c
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

KIND_MOVE = 1
KIND_BUTTON = 2
KIND_KEY = 3
KIND_WHEEL = 4

_HEADER = struct.Struct("<4sHHQQQ")  # 32 octets
_RECORD = struct.Struct("<QHHiii")   # 24 octets


@dataclass(frozen=True)
class Header:
    version: int
    qpc_freq: int
    qpc_start: int
    unix_start_ms: int


@dataclass(frozen=True)
class Event:
    """Un événement d'entrée.

    t_ms : ms depuis le début de la session (qpc - qpc_start).
    kind : KIND_* ; a/b : payload (move -> dx/dy, button -> id/down,
           key -> vk/down, wheel -> delta).
    """

    t_ms: float
    kind: int
    flags: int
    a: int
    b: int
    c: int


def read_header(path: Path | str) -> Header:
    data = Path(path).read_bytes()[: _HEADER.size]
    magic, version, _hsize, qpc_freq, qpc_start, unix_ms = _HEADER.unpack(data)
    if magic != b"ATSI":
        raise ValueError(f"not an atsi file: {path}")
    return Header(version, qpc_freq, qpc_start, unix_ms)


def events(path: Path | str) -> Iterator[Event]:
    data = Path(path).read_bytes()
    magic, _version, _hsize, qpc_freq, qpc_start, _unix_ms = _HEADER.unpack_from(data, 0)
    if magic != b"ATSI":
        raise ValueError(f"not an atsi file: {path}")
    for off in range(_HEADER.size, len(data) - _RECORD.size + 1, _RECORD.size):
        qpc, kind, flags, a, b, c = _RECORD.unpack_from(data, off)
        yield Event((qpc - qpc_start) * 1000.0 / qpc_freq, kind, flags, a, b, c)


def load(path: Path | str) -> tuple[Header, list[Event]]:
    return read_header(path), list(events(path))
