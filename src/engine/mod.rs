pub mod decoding;
pub mod encoding;
pub mod receiving;
pub mod sending;

// TODO
// Potential Dead load with tokio::mpsc or flume::
mod bus_flume;
// mod bus_tokio;

pub use bus_flume::{Bus, BusInterface};
// pub use bus_tokio::{Bus, BusInterface};

use std::net::SocketAddr;
use tokio::time::{Duration, Instant};

use crate::protocol::wire::frames::{DataFrame, ParsedDataFrame};
use derive_more::{self, Debug};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BusAddress {
    SenderSocket,
    ReceiverSocket,
    FrameEncoder(u32, SocketAddr),
    FrameDecoder(u32),
}

#[derive(derive_more::From, derive_more::TryInto, Debug)]
pub enum BusMessage<const INFO_LENGTH: usize> {
    SendingOrder(SendingOrder),
    ReceivingChunkReport((u32, ReceivingChunkReport)),
    SendingData((SocketAddr, DataFrame<INFO_LENGTH>)),
    ReceivingData(ParsedDataFrame<INFO_LENGTH>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ReceivingChunkReport {
    WantNext(u32),
    Finished(u32),
}

impl Ord for ReceivingChunkReport {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (ReceivingChunkReport::Finished(a), ReceivingChunkReport::Finished(b)) => a.cmp(b),
            (ReceivingChunkReport::Finished(_), ReceivingChunkReport::WantNext(_)) => {
                std::cmp::Ordering::Greater
            }
            (ReceivingChunkReport::WantNext(_), ReceivingChunkReport::Finished(_)) => {
                std::cmp::Ordering::Less
            }
            (ReceivingChunkReport::WantNext(a), ReceivingChunkReport::WantNext(b)) => a.cmp(b),
        }
    }
}
impl PartialOrd for ReceivingChunkReport {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.cmp(other).into()
    }
}

#[derive(Debug)]
pub struct SendingOrder {
    pub chunk_id: u32,
    pub sending_interval: Option<Duration>,
    pub time_stamp: Instant,
    pub offset_next: u32,
    pub offset_no_more_than: u32,
    pub close_now: bool,
}

// use dashmap::{DashMap, DashSet};

// struct DownloaderControlBlock<const INFO_LENGTH: usize> {
//     pub latest_want: ReceivingChunkReport,
//     pub data_sender: flume::Sender<DataFrame<INFO_LENGTH>>,
// }

// pub struct Downloader<S: UdpSocketLike, const INFO_LENGTH: usize> {
//     socket: S,
//     report_sender: flume::Sender<(u32, ReceivingChunkReport)>,
//     report_receiver: flume::Receiver<(u32, ReceivingChunkReport)>,
//     contol_block: DashMap<u32, DownloaderControlBlock<INFO_LENGTH>>,
// }

// impl<S: UdpSocketLike, const INFO_LENGTH: usize> Downloader<S, INFO_LENGTH> {
//     pub fn new(socket: S) -> Self {
//         let (report_sender, report_receiver) = flume::unbounded();
//         Self {
//             socket,
//             report_sender,
//             report_receiver,
//             contol_block: DashMap::new(),
//         }
//     }

//     pub fn register(
//         &mut self,
//         chunk_id: u32,
//     ) -> (
//         flume::Receiver<DataFrame<INFO_LENGTH>>,
//         flume::Sender<(u32, ReceivingChunkReport)>,
//     ) {
//         let report_sender = self.report_sender.clone();
//         let (data_sender, data_receiver) = flume::bounded(32);
//         let control_block = DownloaderControlBlock {
//             latest_want: ReceivingChunkReport::WantNext(0),
//             data_sender,
//         };
//         self.contol_block.insert(chunk_id, control_block);
//         (data_receiver, report_sender)
//     }

//     pub async fn try_download<FR>(&mut self, chunk_id: u32) -> Option<Vec<u8>>
//     where
//         FR: FrameReceiver<INFO_LENGTH>,
//     {
//         let (data_receiver, reporter) = self.register(chunk_id);
//         let mut receiving_chunk =
//             receiving::ReceivingChunk::<INFO_LENGTH>::new(chunk_id, data_receiver, reporter);
//         receiving_chunk.run::<FR>().await
//     }
// }
