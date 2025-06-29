use anyhow::{Context, anyhow};
use rust_mc_proto::{self, DataReader, MCConnTcp, MinecraftConnection, Packet};
use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, RwLock},
    thread,
};

fn main() -> anyhow::Result<()> {}
