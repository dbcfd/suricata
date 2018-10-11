use packet_ipc::client::Client as IpcClient;

//IPC Integration
pub type SCPacketExtRelease = extern "C" fn(user: *mut u8);
pub enum Packet {}
pub type SCSetPacketDataFunc = extern "C" fn(
    packet: *mut Packet,
    pktdata: *const libc::uint8_t,
    pktlen: libc::uint32_t,
    linktype: libc::uint32_t,
    ts: libc::timeval,
    release: SCPacketExtRelease,
    user: *mut libc::c_void
) -> libc::int32_t;

extern "C" fn ipc_packet_callback(user: *mut u8) {
    if user != std::ptr::null_mut() {
        unsafe {
            let packet = std::mem::transmute::<*mut u8, *mut packet_ipc::packet::Packet>(user);
            let _packet = Box::from_raw(packet);
            std::mem::drop(_packet);
        }
    }
}

#[no_mangle]
pub extern "C" fn ipc_populate_packets(ipc: *mut IpcClient, packets: *mut *mut Packet, len: libc::uint64_t) -> libc::int64_t {
    let sc = unsafe {
        if let Some(sc) = crate::core::SC {
            sc
        } else {
            return 0;
        }
    };

    if ipc.is_null() {
        SCLogNotice!("IPC passed to ipc_populate_packets was null");
        return -1;
    }

    if packets.is_null() {
        SCLogNotice!("Packets passed to ipc_populate_packets was null");
        return -1;
    }

    if len == 0 {
        SCLogNotice!("No packets requested");
        return -1;
    }

    match unsafe { (*ipc).receive_packets(len as usize) } {
        Err(_) => {
            SCLogNotice!("Failed to receive packets in ipc_populate_packets");
            return -1;
        }
        Ok(None) => {
            SCLogInfo!("IPC connection closed");
            return 0;
        }
        Ok(Some(mut ipc_packets)) => {
            if ipc_packets.is_empty() {
                SCLogInfo!("IPC connection closed");
                return 0;
            } else {
                SCLogDebug!("Received {} packets", ipc_packets.len());
                let packets_returned = ipc_packets.len();
                let mut packet_offset = (packets_returned - 1) as isize;

                while let Some(packet) = ipc_packets.pop() {
                    let raw_p = unsafe { *packets.offset(packet_offset) };
                    if raw_p.is_null() {
                        SCLogNotice!("Packet passed to ipc_populate_packets was null");
                        return -1;
                    }
                    if let Ok(dur) = packet.timestamp().duration_since(std::time::UNIX_EPOCH) {
                        let seconds = dur.as_secs() as i64;
                        let micros = dur.subsec_micros() as i32;
                        let ts = libc::timeval {
                            tv_sec: seconds,
                            tv_usec: micros
                        };
                        let data = packet.data();
                        if (sc.SetPacketData)(
                            raw_p,
                            data.as_ptr(),
                            data.len() as u32,
                            1, //should probably come with the packet
                            ts,
                            ipc_packet_callback,
                            packet.into_raw() as *mut libc::c_void
                        ) != 0 {
                            SCLogNotice!("Failed to set packet data");
                            return -1;
                        }
                        packet_offset -= 1;
                    } else {
                        SCLogNotice!("Unable to convert timestamp to timeval in ipc_populate_packets");
                        return -1;
                    }
                }
                return packets_returned as libc::int64_t;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn create_ipc_client(server_name: *const libc::c_char, client: *mut *mut IpcClient) -> libc::uint32_t {
    let server = unsafe { std::ffi::CStr::from_ptr(server_name) };
    if let Ok(s) = server.to_str() {
        if let Ok(ipc) = IpcClient::new(s.to_string()) {
            let raw = Box::into_raw(Box::new(ipc));
            unsafe { *client = raw };
            1
        } else {
            0
        }
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn release_ipc_client(ipc: *mut IpcClient) {
    let _ipc: Box<IpcClient> = unsafe { Box::from_raw(ipc) };
}