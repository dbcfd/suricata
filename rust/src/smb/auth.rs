/* Copyright (C) 2018 Open Information Security Foundation
 *
 * You can copy, redistribute or modify this Program under the terms of
 * the GNU General Public License version 2 as published by the Free
 * Software Foundation.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * version 2 along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA
 * 02110-1301, USA.
 */

use crate::kerberos::*;

use crate::smb::ntlmssp_records::*;
use crate::smb::smb::*;

use der_parser;
use nom::*;

fn parse_secblob_get_spnego(blob: &[u8]) -> IResult<&[u8], &[u8]> {
    let (rem, base_o) = der_parser::parse_der(blob)?;
    SCLogDebug!("parse_secblob_get_spnego: base_o {:?}", base_o);
    let d = if let Ok(d) = base_o.content.as_slice() {
        d
    } else {
        return Err(nom::Err::Error(error_position!(blob, ErrorKind::Custom(
            SECBLOB_NOT_SPNEGO
        ))));
    };
    let (next, o) = der_parser::parse_der_oid(d)?;
    SCLogDebug!("parse_secblob_get_spnego: sub_o {:?}", o);

    let oid = if let Ok(oid) = o.content.as_oid() {
        oid
    } else {
        return Err(nom::Err::Error(error_position!(blob, ErrorKind::Custom(SECBLOB_NOT_SPNEGO))));
    };
    SCLogDebug!("oid {}", oid.to_string());

    match oid.to_string().as_str() {
        "1.3.6.1.5.5.2" => {
            SCLogDebug!("SPNEGO {}", oid);
        }
        _ => {
            return Err(nom::Err::Error(error_position!(blob, ErrorKind::Custom(SECBLOB_NOT_SPNEGO))));
        }
    }

    SCLogDebug!("parse_secblob_get_spnego: next {:?}", next);
    SCLogDebug!("parse_secblob_get_spnego: DONE");
    Ok( (rem, next) )
}

fn parse_secblob_spnego_start(blob: &[u8]) -> IResult<&[u8], &[u8]> {
    let (rem, o) = der_parser::parse_der(blob)?;
    let d = if let Ok(d) = o.content.as_slice() {
        SCLogDebug!("d: next data len {}", d.len());
        d
    } else {
        return Err(nom::Err::Error(error_position!(blob, ErrorKind::Custom(SECBLOB_NOT_SPNEGO))));
    };
    Ok( (rem, d) )
}

pub struct SpnegoRequest {
    pub krb: Option<Kerberos5Ticket>,
    pub ntlmssp: Option<NtlmsspData>,
}

fn parse_secblob_spnego(blob: &[u8]) -> Option<SpnegoRequest> {
    let mut have_ntlmssp = false;
    let mut have_kerberos = false;
    let mut kticket: Option<Kerberos5Ticket> = None;
    let mut ntlmssp: Option<NtlmsspData> = None;

    let o = if let Ok( (_, o)) = der_parser::parse_der_sequence(blob) {
        o
    } else {
        return None;
    };
    for s in o {
        SCLogDebug!("s {:?}", s);

        let n = if let Ok(s) = s.content.as_slice() {
            s
        } else {
            continue;
        };
        let o = if let Ok( (_, x) ) = der_parser::parse_der(n) {
            x
        } else {
            continue;
        };
        SCLogDebug!("o {:?}", o);
        match o.content {
            der_parser::DerObjectContent::Sequence(ref seq) => {
                for se in seq {
                    SCLogDebug!("SEQ {:?}", se);
                    match se.content {
                        der_parser::DerObjectContent::OID(ref oid) => {
                            SCLogDebug!("OID {:?}", oid);
                            match oid.to_string().as_str() {
                                "1.2.840.48018.1.2.2" => {
                                    SCLogDebug!("Microsoft Kerberos 5");
                                }
                                "1.2.840.113554.1.2.2" => {
                                    SCLogDebug!("Kerberos 5");
                                    have_kerberos = true;
                                }
                                "1.2.840.113554.1.2.2.1" => {
                                    SCLogDebug!("krb5-name");
                                }
                                "1.2.840.113554.1.2.2.2" => {
                                    SCLogDebug!("krb5-principal");
                                }
                                "1.2.840.113554.1.2.2.3" => {
                                    SCLogDebug!("krb5-user-to-user-mech");
                                }
                                "1.3.6.1.4.1.311.2.2.10" => {
                                    SCLogDebug!("NTLMSSP");
                                    have_ntlmssp = true;
                                }
                                "1.3.6.1.4.1.311.2.2.30" => {
                                    SCLogDebug!("NegoEx");
                                }
                                _ => {
                                    SCLogDebug!("unexpected OID {:?}", oid);
                                }
                            }
                        }
                        _ => {
                            SCLogDebug!("expected OID, got {:?}", se);
                        }
                    }
                }
            }
            der_parser::DerObjectContent::OctetString(ref os) => {
                if have_kerberos {
                    if let Ok( (_, t) ) = parse_kerberos5_request(os) {
                        kticket = Some(t)
                    }
                }

                if have_ntlmssp && kticket == None {
                    SCLogDebug!("parsing expected NTLMSSP");
                    ntlmssp = parse_ntlmssp_blob(os);
                }
            }
            _ => {}
        }
    }

    let s = SpnegoRequest {
        krb: kticket,
        ntlmssp: ntlmssp,
    };
    Some(s)
}

#[derive(Debug, PartialEq)]
pub struct NtlmsspData {
    pub host: Vec<u8>,
    pub user: Vec<u8>,
    pub domain: Vec<u8>,
    pub version: Option<NTLMSSPVersion>,
}

/// take in blob, search for the header and parse it
fn parse_ntlmssp_blob(blob: &[u8]) -> Option<NtlmsspData> {
    let mut ntlmssp_data: Option<NtlmsspData> = None;

    SCLogDebug!("NTLMSSP {:?}", blob);

    if let Ok( (_, nd) ) = parse_ntlmssp(blob) {
        SCLogDebug!(
            "NTLMSSP TYPE {}/{} nd {:?}",
            nd.msg_type,
            &ntlmssp_type_string(nd.msg_type),
            nd
        );
        match nd.msg_type {
            NTLMSSP_NEGOTIATE => {}
            NTLMSSP_AUTH => {
                if let Ok( (_, ad) ) = parse_ntlm_auth_record(nd.data) {
                    SCLogDebug!("auth data {:?}", ad);
                    let mut host = ad.host.to_vec();
                    host.retain(|&i| i != 0x00);
                    let mut user = ad.user.to_vec();
                    user.retain(|&i| i != 0x00);
                    let mut domain = ad.domain.to_vec();
                    domain.retain(|&i| i != 0x00);

                    let d = NtlmsspData {
                        host: host,
                        user: user,
                        domain: domain,
                        version: ad.version,
                    };
                    ntlmssp_data = Some(d);
                }
            }
            _ => {}
        }
    }
    return ntlmssp_data;
}

// if spnego parsing fails try to fall back to ntlmssp
pub fn parse_secblob(blob: &[u8]) -> Option<SpnegoRequest> {
    if let Ok( (_, spnego) ) = parse_secblob_get_spnego(blob) {
        if let Ok((_, spnego_start)) = parse_secblob_spnego_start(spnego) {
            parse_secblob_spnego(spnego_start)
        } else {
            parse_ntlmssp_blob(blob).map(|n| {
                SpnegoRequest {
                    krb: None,
                    ntlmssp: Some(n),
                }
            })
        }
    } else {
        parse_ntlmssp_blob(blob).map(|n| {
            SpnegoRequest {
                krb: None,
                ntlmssp: Some(n),
            }
        })
    }
}
