use anyhow::{Context, anyhow};
use rust_mc_proto::{self, DataReader, MCConnTcp, MinecraftConnection, Packet};
use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, RwLock},
    thread,
};

fn main() -> anyhow::Result<()> {
    let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 52));
    let port = 2345;
    let listener =
        TcpListener::bind(SocketAddr::new(ip, port)).expect("Error. failed to bind to the address");

    println!("start");

    for streams in listener.incoming() {
        println!("connect");
        let stream = streams.unwrap();
        let mut client_bind = MCConnTcp::new(stream);
        let mut server_bind = MCConnTcp::connect((Ipv4Addr::new(100, 117, 205, 101), 25555))?;

        let handshake = client_bind.read_packet()?;
        server_bind.write_packet(&handshake)?;

        let login_start = client_bind.read_packet()?;
        server_bind.write_packet(&login_start)?;

        // set compression
        let set_compression = server_bind.read_packet()?;
        let threshold = set_compression.clone().read_usize_varint()?;
        server_bind.set_compression(Some(threshold));
        client_bind.write_packet(&set_compression)?;
        client_bind.set_compression(Some(threshold));

        let login_success = server_bind.read_packet()?;
        client_bind.write_packet(&login_success)?;
        println!("login_success");

        // let login_acknowledged = client_bind.read_packet()?;
        // server_bind.write_packet(&login_acknowledged)?;
        // println!("login_acknowledged: {:?}", login_acknowledged);

        let mut client_bind_1 = client_bind.try_clone()?;
        let mut server_bind_1 = server_bind.try_clone()?;

        thread::spawn(move || {
            let mut sended_ids = HashSet::<u8>::new();
            loop {
                let mut packet = client_bind_1.read_packet().expect("msg1");
                let id = packet.id();
                if !sended_ids.contains(&id) {
                    println!("C> |{:02x}", id);
                    sended_ids.insert(id);
                }
                if id == 0x0d {
                    println!("{:?}", packet.get_bytes());
                    // println!("{}", packet.read_byte().unwrap());
                    // println!("{}", packet.read_varint().unwrap());
                    // println!("{}", packet.read_boolean().unwrap());
                    // println!("{}", packet.read_byte().unwrap());
                    // println!("{}", packet.read_varint().unwrap());
                    // println!("{}", packet.read_boolean().unwrap());
                    // println!("{}", packet.read_boolean().unwrap());
                    // println!("{}", packet.read_varint().unwrap());
                }
                server_bind_1.write_packet(&packet).expect("msg2");
            }
        });

        thread::spawn(move || {
            let mut sended_ids = HashSet::<u8>::new();
            loop {
                let packet = server_bind.read_packet().expect("msg3");
                let id = packet.id();
                if !sended_ids.contains(&id) {
                    println!(" <S|{:02x}", id);
                    sended_ids.insert(id);
                }
                client_bind.write_packet(&packet).expect("msg4");
            }
        });
    }
    Ok(())
}
