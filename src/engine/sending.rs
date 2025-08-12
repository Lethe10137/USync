use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use super::{BusAddress, BusInterface, BusMessage, SendingOrder};
use crate::constants::MTU;
use crate::protocol::coding::FrameSender;
use crate::protocol::wire::encoding::{PacketExt, ParsedPacket, parse_packet};
use crate::protocol::wire::frames::ParsedFrameVariant;
use crate::protocol::wire::packets::ParsedPacketVariant;
use crate::protocol::wire::{frames::DataFrame, packets::DataPacket};
use crate::transmission::UdpSocketLike;

use bytes::Bytes;

use tokio::time::Instant;

pub struct SendingSocket<S: UdpSocketLike, const INFO_LENGTH: usize> {
    socket: S,
    bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
}

fn build_sending_order<const INFO_LENGTH: usize>(
    packet: ParsedPacket<INFO_LENGTH>,
    socket_addr: SocketAddr,
) -> Option<HashMap<BusAddress, SendingOrder>> {
    let ParsedPacketVariant::TicketPacket { .. } = packet.specific_packet_header else {
        return None;
    };
    let mut orders = HashMap::new();
    let mut sending_interval = None;
    for frame in packet.frames {
        match frame {
            ParsedFrameVariant::GetChunk(header) => {
                let chunk_id: u32 = header.chunk_id.into();
                let next_recieve: u32 = header.next_receive_offset.into();
                let receive_window: u32 = header.receive_window_frames.into();

                let order = SendingOrder {
                    chunk_id,
                    sending_interval,
                    time_stamp: Instant::now(),
                    offset_next: next_recieve,
                    offset_no_more_than: next_recieve + receive_window,
                    close_now: receive_window == 0,
                };
                orders.insert(BusAddress::FrameEncoder(chunk_id, socket_addr), order);
            }
            ParsedFrameVariant::RateLimit(header) => {
                let rate_limit = u32::from(header.desired_max_kbps);
                sending_interval = Duration::from_millis(8)
                    .mul_f32((MTU + 20) as f32)
                    .div_f64(rate_limit as f64)
                    .into();
            }
            _ => {}
        }
    }

    orders.into()
}

impl<S: UdpSocketLike, const INFO_LENGTH: usize> SendingSocket<S, INFO_LENGTH> {
    pub fn new(
        socket: S,
        bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
    ) -> Self {
        Self {
            socket,
            bus_interface,
        }
    }

    pub async fn run<FS>(mut self)
    where
        FS: FrameSender<INFO_LENGTH> + Send + 'static,
    {
        let mut buffer = [0u8; 65537];
        loop {
            tokio::select! {
                Ok((length, sock_addr)) = self.socket.recv_from(&mut buffer) => {
                    let packet = Bytes::from(Vec::from(&buffer[0..length]));
                    if let Some(parsed_packet) = parse_packet::<INFO_LENGTH>(packet)
                        .inspect_err(|err| {dbg!(err);})
                        .ok().map(
                        |parsed_packet| build_sending_order(parsed_packet, sock_addr).into_iter().flatten()
                    ){
                        for (addr, order) in parsed_packet.into_iter(){
                            if let Err(order) = self.bus_interface.send(addr.clone(), order).await{
                                let start_order = order.unwrap();
                                if start_order.close_now {continue;}
                                eprintln!("Init encoder for chunk {:?}, addr {:?}", start_order.chunk_id, &addr);
                                let bus = self.bus_interface.get_bus();
                                super::encoding::spawn::<FS, INFO_LENGTH>(start_order, bus, sock_addr, addr).await;
                            }
                        }
                    }
                },

                Some((addr, frame)) = self.bus_interface.recv::<(SocketAddr, DataFrame<INFO_LENGTH>)>() => {
                    let packet = DataPacket::from(frame).build();
                    self.socket.send_to(packet.as_slice(), addr).await.ok();
                },

                else => {
                    break;
                }
            }
        }
    }
}
