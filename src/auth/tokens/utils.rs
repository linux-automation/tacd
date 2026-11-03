// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2026 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this library; if not, see <https://www.gnu.org/licenses/>.

use std::io::{Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Serialize, Serializer};

use super::DeserializationError;

pub(super) fn to_unix_ts(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

pub(super) fn from_unix_ts(ts: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(ts)
}

pub(super) fn whole_second(time: SystemTime) -> SystemTime {
    from_unix_ts(to_unix_ts(time))
}

pub(super) fn serialize_system_time<S: Serializer>(
    time: &SystemTime,
    ser: S,
) -> Result<S::Ok, S::Error> {
    let javascript_ts = to_unix_ts(*time) * 1000;
    javascript_ts.serialize(ser)
}

pub(super) fn read_u8<R: Read>(reader: &mut R) -> Result<u8, DeserializationError> {
    let mut buf = [0u8];
    reader
        .read_exact(&mut buf)
        .map_err(|_| DeserializationError::TooFewBytes)?;
    Ok(buf[0])
}

pub(super) fn read_u16<R: Read>(reader: &mut R) -> Result<u16, DeserializationError> {
    let mut buf = [0u8; 2];
    reader
        .read_exact(&mut buf)
        .map_err(|_| DeserializationError::TooFewBytes)?;
    Ok(u16::from_be_bytes(buf))
}

pub(super) fn read_u32<R: Read>(reader: &mut R) -> Result<u32, DeserializationError> {
    let mut buf = [0u8; 4];
    reader
        .read_exact(&mut buf)
        .map_err(|_| DeserializationError::TooFewBytes)?;
    Ok(u32::from_be_bytes(buf))
}

pub(super) fn read_u64<R: Read>(reader: &mut R) -> Result<u64, DeserializationError> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|_| DeserializationError::TooFewBytes)?;
    Ok(u64::from_be_bytes(buf))
}

pub(super) fn read_eof<R: Read>(reader: &mut R) -> Result<(), DeserializationError> {
    match read_u8(reader) {
        Ok(_) => Err(DeserializationError::UnusedBytes),
        Err(_) => Ok(()),
    }
}

pub(super) fn write_u8<W: Write>(writer: &mut W, val: u8) {
    writer.write_all(&[val]).unwrap();
}

pub(super) fn write_u16<W: Write>(writer: &mut W, val: u16) {
    writer.write_all(&val.to_be_bytes()).unwrap();
}

pub(super) fn write_u32<W: Write>(writer: &mut W, val: u32) {
    writer.write_all(&val.to_be_bytes()).unwrap();
}

pub(super) fn write_u64<W: Write>(writer: &mut W, val: u64) {
    writer.write_all(&val.to_be_bytes()).unwrap();
}
