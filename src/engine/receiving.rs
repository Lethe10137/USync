use super::{BusAddress, BusInterface, BusMessage, ReceivingChunkReport};
use crate::protocol::wire::encoding::{PacketExt, parse_packet};
use crate::protocol::wire::frames::ParsedFrameVariant;
use crate::protocol::wire::packets::TicketPacket;
use crate::transmission::UdpSocketLike;
use crate::util::Compare;
use bytes::Bytes;
use owo_colors::*;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use tokio::time::{Duration, interval};

#[derive(Default)]
struct Reporter {
    activate_data: HashMap<u32, ReceivingChunkReport>,
    exiting_data: VecDeque<HashMap<u32, ReceivingChunkReport>>,
}

impl Reporter {
    fn is_empty(&self) -> bool {
        let exited = self.exiting_data.iter().map(|s| s.len()).sum();
        dbg!(exited);
        self.activate_data.is_empty() && 0usize == exited
    }

    fn update(&mut self, chunk_id: u32, report: ReceivingChunkReport) {
        self.activate_data
            .entry(chunk_id)
            .and_modify(|x| x.cmax(report.clone()))
            .or_insert_with_key(|_| report);
    }

    fn generate(&mut self, rate_kbps: u32) -> TicketPacket {
        if self.exiting_data.len() >= 3 {
            self.exiting_data.pop_back();
        }

        self.exiting_data.push_front(
            self.activate_data
                .extract_if(|_k, v| *v >= ReceivingChunkReport::Finished(0))
                .collect(),
        );

        self.activate_data
            .iter()
            .chain(self.exiting_data.iter().flat_map(|s| s.iter()))
            .fold(
                TicketPacket::new().set_rate_limit(rate_kbps),
                |packet: TicketPacket, (chunk_id, result)| match result {
                    ReceivingChunkReport::WantNext(n) => {
                        packet.set_get_chunk(*chunk_id, *n, 8192.max(*n / 5))
                    }
                    ReceivingChunkReport::Finished(n) => packet.set_get_chunk(*chunk_id, *n, 0),
                },
            )
    }
}

pub struct ReceivingSocket<S: UdpSocketLike, const INFO_LENGTH: usize> {
    socket: S,
    bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
}
impl<S: UdpSocketLike, const INFO_LENGTH: usize> ReceivingSocket<S, INFO_LENGTH> {
    pub fn new(
        socket: S,
        bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
    ) -> Self {
        Self {
            socket,
            bus_interface,
        }
    }

    pub async fn run(mut self, server_addr: SocketAddr) {
        let mut buffer = [0u8; 65537];
        let mut reporter = Reporter::default();
        let mut ticker = interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                biased;

                _ = ticker.tick() => {
                    eprintln!("{}", "Tick".yellow());
                    if !reporter.is_empty() {
                        let packet = reporter.generate(40960).build(); // 40Mbps
                        if self.socket.send_to(packet.as_slice(), server_addr).await.is_err(){
                            eprintln!("{}", "Failed to send report to server!".red());
                            break;
                        }
                    }
                },

                Ok((length, _)) = self.socket.recv_from(&mut buffer) => {
                    let packet = Bytes::from(Vec::from(&buffer[0..length]));
                    if let Ok(packet) = parse_packet::<INFO_LENGTH>(packet){
                        for frame in packet.frames{
                            if let ParsedFrameVariant::Data(data_frame) = frame{
                                let _ = self.bus_interface.send(BusAddress::FrameDecoder(data_frame.chunk_id), data_frame).await;
                            }
                        }
                    }
                },

                Some((chunk_id, report)) = self.bus_interface.recv::<(u32,  ReceivingChunkReport)>() => {
                    reporter.update(chunk_id, report);
                },



                else => {
                    eprintln!("{}", "SenderSocketexit".red());
                    break;
                }
            }
        }
    }
}
