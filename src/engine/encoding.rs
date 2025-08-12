use crate::protocol::{coding::FrameSender, wire::frames::DataFrame};
use crate::util::Compare;
use crate::util::file::{CHUNK_INDEX, mmap_segment};
use crate::util::timer::{SenderTimer, SenderTimerOutput};
use bytes::Bytes;
use memmap2::Mmap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

use super::{Bus, BusAddress, BusInterface, BusMessage, SendingOrder};

use crate::util::timer_logger::print_relative_time;

pub async fn spawn<FS, const INFO_LENGTH: usize>(
    start_order: SendingOrder,
    bus: Arc<Bus<BusAddress, BusMessage<INFO_LENGTH>>>,
    sock_addr: SocketAddr,
    bus_addr: BusAddress,
) where
    FS: FrameSender<INFO_LENGTH> + std::marker::Send + 'static,
{
    let chunk_info = CHUNK_INDEX
        .get()
        .and_then(|index| index.get(start_order.chunk_id));
    if chunk_info.is_none() {
        return;
    }
    let chunk_info = chunk_info.unwrap();

    let chunk_data = mmap_segment(chunk_info.0, chunk_info.1, chunk_info.2).unwrap();

    let bus_interface = bus.register(bus_addr);
    let encoder: ChunkEncoder<FS, INFO_LENGTH> =
        ChunkEncoder::new(chunk_data, start_order, bus_interface, sock_addr).await;

    tokio::spawn(encoder.run());
}

pub struct ChunkEncoder<FS: FrameSender<INFO_LENGTH>, const INFO_LENGTH: usize> {
    chunk_id: u32,
    encoder: FS,
    transmission_info: [u8; INFO_LENGTH],
    bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
    max_frame_offset: u32,
    max_sent_offset: u32,
    timer: SenderTimer,
    sock_addr: SocketAddr,
}

impl<FS: FrameSender<INFO_LENGTH>, const INFO_LENGTH: usize> ChunkEncoder<FS, INFO_LENGTH>
where
    FS: Send + 'static,
{
    pub async fn new(
        chunk_data: Mmap,
        start_order: SendingOrder,
        bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
        sock_addr: SocketAddr,
    ) -> Self {
        print_relative_time(start_order.chunk_id, "Start init sender", Instant::now());
        let encoder =
            tokio::task::spawn_blocking(move || FS::init(chunk_data, start_order.offset_next))
                .await
                .unwrap();

        let transmission_info = encoder.get_trasmission_info();
        let sender = Self {
            chunk_id: start_order.chunk_id,
            encoder,
            transmission_info,
            bus_interface,
            timer: SenderTimer::new(
                start_order
                    .sending_interval
                    .unwrap_or(Duration::from_millis(20)),
            ),
            max_sent_offset: 0,
            max_frame_offset: start_order.offset_next + start_order.offset_no_more_than,
            sock_addr,
        };
        print_relative_time(start_order.chunk_id, "Finish init sender", Instant::now());
        sender
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(order) = self.bus_interface.recv::<SendingOrder>() => {
                    let now = Instant::now();
                    print_relative_time(self.chunk_id, "Got Order", now);
                    self.timer.set_rate(now, order.sending_interval);
                    self.max_frame_offset.cmax(order.offset_no_more_than);
                    if order.close_now {
                        print_relative_time(self.chunk_id, "FINISH", now);
                        break;
                    }
                },

                output = &mut self.timer => {
                    match output {
                        SenderTimerOutput::Send(x) => {
                            for _ in 0..x{
                                if self.max_sent_offset >= self.max_frame_offset {break;}
                                let (frame_offset, frame) = self.encoder.next_frame();
                                let data_frame = DataFrame::new(self.chunk_id, frame_offset, self.transmission_info, Bytes::from(frame));

                                if self.bus_interface.send(BusAddress::SenderSocket,(self.sock_addr, data_frame )).await.is_err(){
                                    print_relative_time(self.chunk_id, "Can not send", Instant::now());
                                    break;
                                }

                                if frame_offset % 4096 == 0{
                                    print_relative_time(self.chunk_id, format!("Send {frame_offset}").as_str(), Instant::now());
                                }

                                self.max_sent_offset = frame_offset;
                            }
                        },
                        SenderTimerOutput::Close => {
                            print_relative_time(self.chunk_id, "CLOSE", Instant::now());
                            break;
                        }
                    };
                }
            }
        }
    }
}
