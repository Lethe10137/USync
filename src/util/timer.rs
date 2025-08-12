use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::time::Instant;

pub enum SenderTimerOutput {
    Send(usize),
    Close,
}

pub struct SenderTimer {
    interval: Duration,
    sleep_after: Instant,
    exit_after: Instant,
    last_send: Instant,
    waker: Option<Waker>,
}

const STOP_AFTER: Duration = Duration::from_secs(10);
const EXIT_AFTER: Duration = Duration::from_secs(20);
const MAX_BURST: usize = 8;

impl SenderTimer {
    pub fn new(interval: Duration) -> Self {
        let now = Instant::now();
        Self {
            interval,
            sleep_after: now + STOP_AFTER,
            exit_after: now + EXIT_AFTER,
            last_send: now,
            waker: None,
        }
    }

    pub fn set_rate(&mut self, timestamp: Instant, new_interval: Option<Duration>) {
        if let Some(new_interval) = new_interval {
            self.interval = new_interval;
            self.last_send = self.last_send.max(timestamp - new_interval);
        }

        self.sleep_after = self.sleep_after.max(timestamp + STOP_AFTER);
        self.exit_after = self.exit_after.max(timestamp + EXIT_AFTER);

        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl Future for SenderTimer {
    type Output = SenderTimerOutput;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<SenderTimerOutput> {
        self.waker = Some(cx.waker().clone());
        let now = Instant::now();

        if now >= self.exit_after {
            return Poll::Ready(SenderTimerOutput::Close);
        }

        if now >= self.sleep_after {
            let waker_clone = self.waker.as_ref().unwrap().clone();
            let wake_time_clone = self.exit_after;
            tokio::spawn(async move {
                tokio::time::sleep_until(wake_time_clone).await;
                waker_clone.wake();
            });
            return Poll::Pending;
        }

        let min_sendable_time = self.last_send + self.interval;

        if now >= min_sendable_time {
            let can_send_num = (now.duration_since(self.last_send)).div_duration_f64(self.interval);
            if can_send_num > 1.0 {
                let can_send_num = can_send_num.floor();
                let advance = self.interval.mul_f64(can_send_num);
                self.last_send += advance;
                return Poll::Ready(SenderTimerOutput::Send(
                    (can_send_num as usize).min(MAX_BURST),
                ));
            }
        }

        let waker_clone = self.waker.as_ref().unwrap().clone();
        tokio::spawn(async move {
            tokio::time::sleep_until(min_sendable_time).await;
            waker_clone.wake();
        });
        Poll::Pending
    }
}

#[cfg(feature = "slow-tests")]
#[cfg(test)]
mod test {

    use super::super::timer_logger::{PROGRAM_START_TIME, print_relative_time};
    use super::*;
    use tokio::select;

    #[tokio::test]
    async fn clock() {
        println!("start");
        let (tx, rx) = flume::bounded::<Duration>(16);

        let controller = tokio::spawn(async move {
            tokio::time::sleep_until(*PROGRAM_START_TIME + Duration::from_secs(3)).await;
            tx.send(Duration::from_millis(500)).unwrap();

            tokio::time::sleep_until(*PROGRAM_START_TIME + Duration::from_secs(20)).await;
            tx.send(Duration::from_millis(1500)).unwrap();
        });

        let sender = tokio::spawn(async move {
            let mut timer = SenderTimer::new(Duration::from_millis(900));
            let mut cnt = 0;
            let mut sent_times = vec![];

            loop {
                select! {
                    Ok(new_interval) = rx.recv_async() => {
                        timer.set_rate(Instant::now(), new_interval.into());
                    }
                    output = &mut timer => {
                        match output {
                            SenderTimerOutput::Send(x) => {
                                for _ in 0..x {
                                    sent_times.push(print_relative_time(0, format!("send {}", cnt).as_str(), Instant::now()) / 100.0);
                                    cnt += 1;
                                }
                            },
                            SenderTimerOutput::Close => {
                                sent_times.push( print_relative_time(0, "CLOSE", Instant::now()) / 100.0);
                                break;
                            }
                        };
                    }
                }
            }
            sent_times
        });

        controller.await.unwrap();
        let sent_time = sender
            .await
            .unwrap()
            .into_iter()
            .map(|sent_time| ((sent_time).round() as usize) * 100);

        let expected = (0..=2)
            .map(|x| x * 900 + 900)
            .chain((3..=22).map(|x| x * 500 + 1700))
            .chain((23..=29).map(|x| x * 1500 - 14500))
            .chain(std::iter::once(40000));

        for (i, (expected, actual)) in expected.zip(sent_time).enumerate() {
            println!("{} {}ms {}ms", i, expected, actual,);
            assert_eq!(expected, actual);
        }
    }
}
