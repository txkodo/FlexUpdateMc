use rust_mc_proto::{DataReader, DataWriter, MCConnTcp, Packet, ProtocolError};
use uuid::Uuid;

fn main() -> Result<(), ProtocolError> {
    let mut conn = MCConnTcp::connect("100.117.205.101:25555")?; // connecting

    conn.write_packet(&Packet::build(0x00, |packet| {
        // packet.write_u16_varint(771)?; // protocol_version 1.21.6
        packet.write_u16_varint(762)?; // protocol_version 1.19.4
        packet.write_string("100.117.205.101")?; // server_address
        packet.write_unsigned_short(25555)?; // server_port
        packet.write_u8_varint(2) // next_state
    })?)?; // handshake packet
    println!("handshake sent");

    conn.write_packet(&Packet::build(0x00, |packet| {
        packet.write_string("taro1")?;
        packet.write_boolean(true)?;
        packet.write_bytes(&[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ])
    })?)?; // login start
    println!("login start sent");

    // set compression
    let mut packet = conn.read_packet()?;
    println!("set compression: {}", packet.id());
    let threshold = packet.read_usize_varint()?;

    conn.set_compression(Some(threshold));
    // login success
    let mut packet = conn.read_packet()?;
    println!("login success: {}", packet.id());
    let uuid = packet.read_uuid().unwrap();
    println!("uuid: {}", uuid);
    let username = packet.read_string().unwrap();
    println!("username: {}", username);

    // // login acknowledge
    // conn.write_packet(&Packet::empty(0x03))?;
    // println!("login acknowledge");
    loop {}
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
