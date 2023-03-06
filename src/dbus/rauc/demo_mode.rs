// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use super::SlotStatus;

const SLOT_STATUS: &[u8] = br#"
{
  "rootfs_1": {
    "name": "rootfs.1",
    "bundle_compatible": "lxatac-lxatac",
    "bundle_build": "20230222110225",
    "bundle_version": "4.0-0-20230222110225",
    "slot_class": "rootfs",
    "bootname": "system1",
    "activated_count": "3",
    "boot_status": "good",
    "activated_timestamp": "2023-02-22T11:14:25Z",
    "status": "ok",
    "state": "inactive",
    "size": "983375872",
    "installed_count": "3",
    "sha256": "8136d2ecaee989a125e8f27bd05128ade5dc10270b4cd8a564cbdedaa38f274b",
    "bundle_description": "lxatac-core-bundle-base version 1.0-r0",
    "device": "/dev/disk/by-partuuid/8eb3e87e-2b4e-45e3-888b-2f678662862d",
    "installed_timestamp": "2023-02-22T11:14:16Z",
    "fs_type": "ext4"
  },
  "bootloader_0": {
    "bundle_build": "20230222111713",
    "slot_class": "bootloader",
    "bundle_compatible": "lxatac-lxatac",
    "state": "inactive",
    "sha256": "4e840bcf0b498d2aba040a845d0b2329c9b68396802b2214d5256060824a685f",
    "fs_type": "boot-emmc",
    "installed_timestamp": "2023-02-22T11:23:41Z",
    "device": "/dev/mmcblk1",
    "bundle_version": "4.0-0-20230222111713",
    "bundle_description": "lxatac-core-bundle-base version 1.0-r0",
    "status": "ok",
    "size": "1310720",
    "installed_count": "8",
    "name": "bootloader.0"
  },
  "rootfs_0": {
    "bundle_build": "20230222111713",
    "name": "rootfs.0",
    "installed_timestamp": "2023-02-22T11:23:36Z",
    "activated_timestamp": "2023-02-22T11:23:43Z",
    "fs_type": "ext4",
    "installed_count": "5",
    "state": "booted",
    "size": "983465984",
    "sha256": "90bef769359e1ce0a58f10151ff6c7565fb8b1b3955b8cb3dafaa164a7c381fb",
    "activated_count": "5",
    "bundle_version": "4.0-0-20230222111713",
    "status": "ok",
    "boot_status": "good",
    "bundle_compatible": "lxatac-lxatac",
    "bundle_description": "lxatac-core-bundle-base version 1.0-r0",
    "bootname": "system0",
    "device": "/dev/disk/by-partuuid/e82e6873-62cc-46fb-90f0-3e936743fa62",
    "slot_class": "rootfs"
  }
}
"#;

pub fn slot_status() -> SlotStatus {
    serde_json::from_slice(SLOT_STATUS).unwrap()
}
