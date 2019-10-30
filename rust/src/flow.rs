use crate::log::*;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use core::Flow;

#[derive(Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct FlowBit {
    community_id: String,
    flowbit_type: u8,
    pad: [u8; 3],
    idx: u32,
}

#[derive(Serialize, Deserialize)]
struct FlowBits {
    inner: Vec<FlowBit>,
}

#[no_mangle]
pub extern "C" fn rs_serialize_flowbits(flowbits: Vec<FlowBit>, flowbits_file: &str) -> bool {
    let path = PathBuf::from(flowbits_file);
    match File::open(path) {
        Err(e) => {
            SCLogNotice!("Failed to save flowbits, could not open file: {:?}", e)
        },
        Ok(mut f) => {
            match serde_json::to_vec(&FlowBits { inner: flowbits } ) {
                Err(e) => {
                    SCLogNotice!("Failed to save flowbits, could not serialize: {:?}", e);
                }
                Ok(bytes) => {
                    if let Err(e) = f.write_all(&bytes) {
                        SCLogNotice!("Failed to save flowbits, could not write data: {:?}", e);
                    } else {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub type FlowBitsMap = std::collections::HashMap<String, Vec<FlowBit>>;

#[no_mangle]
pub extern "C" fn load_flowbits(flowbits_file: &str) -> FlowBitsMap {
    let path = PathBuf::from(flowbits_file);
    match File::open(path) {
        Err(e) => {
            SCLogNotice!("Failed to open flowbits_file: {:?}", e);
            return FlowBitsMap::default();
        },
        Ok(mut f) => {
            let mut bytes = vec![];
            if let Err(e) = f.read_to_end(&mut bytes) {
                SCLogNotice!("Failed to read flowbits file: {:?}", e);
                return FlowBitsMap::default()
            }
            match serde_json::from_slice::<FlowBits>(bytes.as_slice()) {
                Err(e) => {
                    SCLogNotice!("Failed to read flowbits file: {:?}", e);
                    FlowBitsMap::default()
                }
                Ok(flowbits) => {
                    let mut res = FlowBitsMap::default();
                    for flowbit in flowbits.inner {
                        match res.get_mut(&flowbit.community_id) {
                            None => {
                                res.insert(flowbit.community_id.clone(), vec![flowbit]);
                            },
                            Some(v) => {
                                v.push(flowbit);
                            },
                        }
                    }
                    res
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_flowbits(flowbits: &FlowBitsMap, community_id: &String) -> Vec<FlowBit> {
    flowbits.get(community_id).map(|v| Vec::clone(v)).unwrap_or(vec![])
}
