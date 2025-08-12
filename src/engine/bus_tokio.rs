use dashmap::DashMap;
use owo_colors::OwoColorize;
use std::sync::Arc;
use std::{fmt::Debug, hash::Hash};
use tokio::sync::mpsc::{self, Receiver, Sender};

pub struct Bus<ADDRESS, MESSAGE>
where
    ADDRESS: Eq + Hash + Clone + Debug,
    MESSAGE: Debug,
{
    peers: DashMap<ADDRESS, Sender<MESSAGE>>,
}

impl<ADDRESS, MESSAGE> Bus<ADDRESS, MESSAGE>
where
    ADDRESS: Eq + Hash + Clone + Debug,
    MESSAGE: Debug,
{
    pub fn debug(&self) {
        eprintln!("BUS devices: {}", self.peers.len());
        todo!();
    }

    pub fn new() -> Self {
        Self {
            peers: DashMap::new(),
        }
    }
    pub fn register(self: Arc<Self>, id: ADDRESS) -> BusInterface<ADDRESS, MESSAGE> {
        eprintln!("BUS:   Register {:?}", &id.green());
        let (tx, rx) = mpsc::channel(100);
        self.peers.insert(id.clone(), tx);
        BusInterface {
            address: id,
            bus: Arc::clone(&self),
            receiver: rx,
        }
    }

    // Returns Err iff trying to send to an address that never existed or has been dropped.
    async fn send(&self, to: ADDRESS, msg: MESSAGE) -> Result<(), MESSAGE> {
        if let Some(sender) = self.peers.get(&to) {
            sender.send(msg).await.map_err(|e| e.0)?;
            Ok(())
        } else {
            Err(msg)
        }
    }

    fn unregister(&self, id: ADDRESS) {
        eprintln!("BUS: Unregister {:?}", &id.red());
        self.peers.remove(&id);
    }
}

pub struct BusInterface<ADDRESS, MESSAGE>
where
    ADDRESS: Eq + Hash + Clone + Debug,
    MESSAGE: Debug,
{
    address: ADDRESS,
    bus: Arc<Bus<ADDRESS, MESSAGE>>,
    receiver: Receiver<MESSAGE>,
}

impl<ADDRESS, MESSAGE> BusInterface<ADDRESS, MESSAGE>
where
    ADDRESS: Eq + Hash + Clone + Debug,
    MESSAGE: Debug,
{
    pub async fn send<M>(&self, to: ADDRESS, message: M) -> Result<(), Option<M>>
    where
        M: Into<MESSAGE> + TryFrom<MESSAGE>,
    {
        let message: MESSAGE = message.into();
        // eprintln!("To {:?}: {:?}", &to.magenta(), &message.blue());
        self.bus
            .send(to, message)
            .await
            .map_err(|err| M::try_from(err).ok())
    }

    pub async fn recv<R: TryFrom<MESSAGE>>(&mut self) -> Option<R> {
        self.receiver
            .recv()
            .await
            .and_then(|message| R::try_from(message).ok())
    }

    pub fn get_bus(&self) -> Arc<Bus<ADDRESS, MESSAGE>> {
        self.bus.clone()
    }
}

impl<ADDRESS, MESSAGE> Drop for BusInterface<ADDRESS, MESSAGE>
where
    ADDRESS: Eq + Hash + Clone + Debug,
    MESSAGE: Debug,
{
    fn drop(&mut self) {
        self.bus.unregister(self.address.clone());
    }
}
