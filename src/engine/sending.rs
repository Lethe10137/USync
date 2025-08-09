use crate::protocol::{coding::FrameSender, wire::frames::DataFrame};
use crate::util::timer::{SenderTimer, SenderTimerOutput};
use bytes::Bytes;
use flume::{Receiver, Sender};
use tokio::time::{Duration, Instant};

use crate::util::timer_logger::print_relative_time;

pub struct SendingOrder {
    pub chunk_id: u32,
    pub sending_interval: Option<Duration>,
    pub time_stamp: Instant,
    pub offset_next: u32,
    pub offset_no_more_than: u32,
    pub close_now: bool,
}

pub struct SendingChunk<FS: FrameSender<INFO_LENGTH>, const INFO_LENGTH: usize> {
    chunk_id: u32,
    encoder: FS,
    transmission_info: [u8; INFO_LENGTH],
    order_receiver: Receiver<SendingOrder>,
    data_sender: Sender<DataFrame<INFO_LENGTH>>,
    max_frame_offset: u32,
    max_sent_offset: u32,
    timer: SenderTimer,
}

impl<FS: FrameSender<INFO_LENGTH>, const INFO_LENGTH: usize> SendingChunk<FS, INFO_LENGTH> {
    pub fn new(
        chunk_data: &[u8],
        start_order: SendingOrder,
        order_receiver: Receiver<SendingOrder>,
        data_sender: Sender<DataFrame<INFO_LENGTH>>,
    ) -> Self {
        print_relative_time("Start init sender", Instant::now());
        let encoder = FS::init(chunk_data, start_order.offset_next);
        let transmission_info = encoder.get_trasmission_info();
        let sender = Self {
            chunk_id: start_order.chunk_id,
            encoder,
            transmission_info,
            order_receiver,
            data_sender,
            timer: SenderTimer::new(
                start_order
                    .sending_interval
                    .unwrap_or(Duration::from_millis(20)),
            ),
            max_sent_offset: 0,
            max_frame_offset: start_order.offset_next + start_order.offset_no_more_than,
        };
        print_relative_time("Finish init sender", Instant::now());
        sender
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                Ok(order) = self.order_receiver.recv_async() => {
                    let now = Instant::now();
                    print_relative_time("ORDER", now);
                    // self.timer.set_rate(, new_interval);
                    self.timer.set_rate(now, order.sending_interval);
                    if order.close_now {
                        print_relative_time("FINISH", now);
                        break;
                    }
                },

                output = &mut self.timer => {
                    match output {
                        SenderTimerOutput::Send(x) => {
                            for _ in 0..x{
                                if self.max_sent_offset >= self.max_frame_offset {break;}
                                let (frame_offset, frame) = self.encoder.next_frame();
                                if self.data_sender.send_async(DataFrame::new(self.chunk_id, frame_offset, self.transmission_info, Bytes::from(frame))).await.is_err(){
                                    print_relative_time("Can not send", Instant::now());
                                    break;
                                }
                                print_relative_time(format!("Send {frame_offset}").as_str(), Instant::now());
                                self.max_sent_offset = frame_offset;
                            }
                        },
                        SenderTimerOutput::Close => {
                            print_relative_time("CLOSE", Instant::now());
                            break;
                        }
                    };
                }
            }
        }
    }
}
