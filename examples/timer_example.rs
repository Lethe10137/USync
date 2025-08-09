use tokio::time::{Duration, Instant};
use usync::engine::sending::*;
use usync::protocol::coding::raptorq_code::RaptorqSender;
use usync::protocol::wire::Frame;
use usync::protocol::wire::frames::DataFrame;

async fn stub_receiver(
    send_order: flume::Sender<SendingOrder>,
    receive_packet: flume::Receiver<DataFrame<12>>,
) {
    for i in 0..64 {
        let data = receive_packet.recv_async().await.unwrap();
        let frame_id: u32 = data.header().frame_offset.into();
        let chunk_id: u32 = data.header().chunk_id.into();

        println!("received:  {} - {}", chunk_id, frame_id);

        if i % 16 == 8 {
            send_order
                .send(SendingOrder {
                    chunk_id,
                    sending_interval: Duration::from_millis(1).into(),
                    time_stamp: Instant::now(),
                    offset_next: frame_id + 1,
                    offset_no_more_than: frame_id + 100,
                    close_now: false,
                })
                .unwrap();
        }
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    println!("receiver exit");
}

#[tokio::main]
async fn main() {
    let (send_order, receive_order) = flume::bounded::<SendingOrder>(16);
    let (send_packet, receive_packet) = flume::unbounded::<DataFrame<12>>();

    let start_order = SendingOrder {
        chunk_id: 0x19260817,
        sending_interval: Duration::from_millis(500).into(),
        time_stamp: Instant::now(),
        offset_next: 0,
        offset_no_more_than: 150,
        close_now: false,
    };
    let mut sender = SendingChunk::<RaptorqSender, 12>::new(
        &[0; 65536],
        start_order,
        receive_order,
        send_packet,
    );

    let sender_future = sender.run();
    let receiver_future = stub_receiver(send_order, receive_packet);

    use async_scoped::{Scope, spawner::use_tokio::Tokio};

    unsafe {
        let mut scope = Scope::create(Tokio);
        scope.spawn(sender_future);
        scope.spawn(receiver_future);
    }
    println!("Finish");
}
