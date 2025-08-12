use super::{Bus, BusAddress, BusInterface, BusMessage, ReceivingChunkReport};
use crate::protocol::{coding::FrameReceiver, wire::frames::ParsedDataFrame};
use std::sync::Arc;
use tokio::task::JoinHandle;

pub fn spawn<FR, const INFO_LENGTH: usize>(
    chunk_id: u32,
    bus: Arc<Bus<BusAddress, BusMessage<INFO_LENGTH>>>,
) -> JoinHandle<Option<Vec<u8>>>
where
    FR: FrameReceiver<INFO_LENGTH> + std::marker::Send + 'static,
{
    let bus_interface = bus.register(BusAddress::FrameDecoder(chunk_id));
    let decoder: ChunkDecoder<INFO_LENGTH> = ChunkDecoder::new(chunk_id, bus_interface);

    tokio::spawn(decoder.run::<FR>())
}

pub struct ChunkDecoder<const INFO_LENGTH: usize> {
    chunk_id: u32,
    bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
}

impl<const INFO_LENGTH: usize> ChunkDecoder<INFO_LENGTH> {
    pub fn new(
        chunk_id: u32,
        bus_interface: BusInterface<BusAddress, BusMessage<INFO_LENGTH>>,
    ) -> Self {
        Self {
            chunk_id,
            bus_interface,
        }
    }

    pub async fn run<FR: FrameReceiver<INFO_LENGTH>>(mut self) -> Option<Vec<u8>> {
        self.bus_interface
            .send(
                BusAddress::ReceiverSocket,
                (self.chunk_id, ReceivingChunkReport::WantNext(0)),
            )
            .await
            .ok();

        let first_chunk: ParsedDataFrame<INFO_LENGTH> = self.bus_interface.recv().await?;

        let mut decoder = FR::try_init(&first_chunk.transmission_info)?;

        if let Some(data) = decoder.update(first_chunk.frame_offset, &first_chunk.data) {
            return Some(data);
        }

        drop(first_chunk);

        loop {
            let frame: ParsedDataFrame<INFO_LENGTH> = self.bus_interface.recv().await?;

            if let Some(data) = decoder.update(frame.frame_offset, &frame.data) {
                self.bus_interface
                    .send(
                        BusAddress::ReceiverSocket,
                        (
                            self.chunk_id,
                            ReceivingChunkReport::Finished(decoder.expected_frame_id()),
                        ),
                    )
                    .await
                    .ok();
                return Some(data);
            }
            self.bus_interface
                .send(
                    BusAddress::ReceiverSocket,
                    (
                        self.chunk_id,
                        ReceivingChunkReport::WantNext(decoder.expected_frame_id()),
                    ),
                )
                .await
                .ok();
        }
    }
}
