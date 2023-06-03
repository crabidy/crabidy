use crate::PlaybackMessage;
use crate::ProviderMessage;
use crabidy_core::proto::crabidy::{
    get_update_stream_response::Update as StreamUpdate, InitResponse, PlayState, Queue, QueueTrack,
    Track, TrackPosition,
};
use crabidy_core::ProviderError;
use gstreamer_play::{Play, PlayState as GstPlaystate, PlayVideoRenderer};
use std::sync::Mutex;
use tracing::debug_span;
use tracing::{debug, error, instrument, trace, warn, Instrument};

#[derive(Debug)]
pub struct Playback {
    update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
    provider_tx: flume::Sender<ProviderMessage>,
    pub playback_tx: flume::Sender<PlaybackMessage>,
    playback_rx: flume::Receiver<PlaybackMessage>,
    queue: Mutex<Queue>,
    state: Mutex<GstPlaystate>,
    pub play: Play,
}

impl Playback {
    pub fn new(
        update_tx: tokio::sync::broadcast::Sender<StreamUpdate>,
        provider_tx: flume::Sender<ProviderMessage>,
    ) -> Self {
        let (playback_tx, playback_rx) = flume::bounded(10);
        let queue = Mutex::new(Queue {
            timestamp: 0,
            current_position: 0,
            tracks: Vec::new(),
        });
        let state = Mutex::new(GstPlaystate::Stopped);
        let play = Play::new(None::<PlayVideoRenderer>);
        Self {
            update_tx,
            provider_tx,
            playback_tx,
            playback_rx,
            queue,
            state,
            play,
        }
    }
    #[instrument]
    pub fn run(self) {
        tokio::spawn(async move {
            while let Ok(message) = self.playback_rx.recv_async().in_current_span().await {
                match message {
                    PlaybackMessage::Init { result_tx, span } => {
                        let _e = span.enter();
                        let response = {
                            let queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            let queue_track = QueueTrack {
                                queue_position: queue.current_position,
                                track: queue.current_track(),
                            };
                            trace!("queue_track {:?}", queue_track);
                            debug!("released queue_track lock");
                            let position = TrackPosition {
                                duration: self
                                    .play
                                    .duration()
                                    .map(|t| t.mseconds() as u32)
                                    .unwrap_or(0),
                                position: self
                                    .play
                                    .position()
                                    .map(|t| t.mseconds() as u32)
                                    .unwrap_or(0),
                            };
                            trace!("position {:?}", position);
                            let play_state = {
                                debug!("getting play state lock");
                                match *self.state.lock().unwrap() {
                                    GstPlaystate::Playing => PlayState::Playing,
                                    GstPlaystate::Paused => PlayState::Paused,
                                    GstPlaystate::Stopped => PlayState::Stopped,
                                    GstPlaystate::Buffering => PlayState::Loading,
                                    _ => PlayState::Unspecified,
                                }
                            };
                            trace!("play_state {:?}", play_state);
                            debug!("released play state lock");
                            InitResponse {
                                queue: Some(queue.clone()),
                                queue_track: Some(queue_track),
                                play_state: play_state as i32,
                                volume: self.play.volume() as f32,
                                mute: self.play.is_muted(),
                                position: Some(position),
                            }
                        };
                        trace!("response {:?}", response);
                        result_tx.send(response).unwrap();
                    }
                    PlaybackMessage::Replace { uuids, span } => {
                        let _e = span.enter();
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).in_current_span().await {
                                    all_tracks.push(track);
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).in_current_span().await;
                                all_tracks.extend(tracks);
                            }
                        }
                        trace!("got tracks {:?}", all_tracks);
                        let current = {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.replace_with_tracks(&all_tracks);
                            queue.set_current_position(0);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            queue_update_tx.send(update).unwrap();
                            queue.current_track()
                        };
                        debug!("got current {:?}", current);
                        self.play(current).in_current_span().await;
                    }

                    PlaybackMessage::Queue { uuids, span } => {
                        let _e = span.enter();
                        debug!("queing");
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).in_current_span().await {
                                    all_tracks.push(track);
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).in_current_span().await;
                                all_tracks.extend(tracks);
                            }
                        }
                        trace!("got tracks {:?}", all_tracks);
                        {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.queue_tracks(&all_tracks);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            if let Err(err) = queue_update_tx.send(update) {
                                error!("{:?}", err)
                            }
                        }
                        debug!("que lock released");
                    }

                    PlaybackMessage::Append { uuids, span } => {
                        let _e = span.enter();
                        debug!("appending");
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).in_current_span().await {
                                    all_tracks.push(track);
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).in_current_span().await;
                                all_tracks.extend(tracks);
                            }
                        }
                        trace!("got tracks {:?}", all_tracks);
                        {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.append_tracks(&all_tracks);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            if let Err(err) = queue_update_tx.send(update) {
                                error!("{:?}", err)
                            }
                        }
                        debug!("queue lock released");
                    }

                    PlaybackMessage::Remove { positions, span } => {
                        let _e = span.enter();
                        debug!("removing");
                        let track = {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            let track = queue.remove_tracks(&positions);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            queue_update_tx.send(update).unwrap();
                            track
                        };
                        debug!("queue lock released");
                        self.play(track).in_current_span().await;
                    }

                    PlaybackMessage::Insert {
                        position,
                        uuids,
                        span,
                    } => {
                        let _e = span.enter();
                        debug!("inserting");
                        let mut all_tracks = Vec::new();
                        for uuid in uuids {
                            if is_track(&uuid) {
                                if let Ok(track) = self.get_track(&uuid).in_current_span().await {
                                    all_tracks.push(track);
                                }
                            } else {
                                let tracks = self.flatten_node(&uuid).in_current_span().await;
                                all_tracks.extend(tracks);
                            }
                        }
                        trace!("got tracks {:?}", all_tracks);
                        {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.insert_tracks(position, &all_tracks);
                            let queue_update_tx = self.update_tx.clone();
                            let update = StreamUpdate::Queue(queue.clone());
                            queue_update_tx.send(update).unwrap();
                        }
                        debug!("queue lock released");
                    }

                    PlaybackMessage::SetCurrent {
                        position: queue_position,
                        span,
                    } => {
                        let _e = span.enter();
                        debug!("setting current");
                        let track = {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.set_current_position(queue_position);
                            queue.current_track()
                        };
                        debug!("quue lock released and  got current {:?}", track);
                        self.play(track).in_current_span().await;
                    }

                    PlaybackMessage::TogglePlay { span } => {
                        let _e = span.enter();
                        debug!("toggling play");
                        {
                            let state = self.state.lock().unwrap();
                            debug!("got state lock");
                            if *state == GstPlaystate::Playing {
                                self.play.pause();
                            } else {
                                self.play.play();
                            }
                        }
                        debug!("state lock released");
                    }

                    PlaybackMessage::Stop { span } => {
                        let _e = span.enter();
                        debug!("stopping");
                        self.play.stop();
                    }

                    PlaybackMessage::ChangeVolume { delta, span } => {
                        let _e = span.enter();
                        debug!("changing volume");
                        let volume = self.play.volume();
                        debug!("got volume {:?}", volume);
                        self.play.set_volume(volume + delta as f64);
                    }

                    PlaybackMessage::ToggleMute { span } => {
                        let _e = span.enter();
                        debug!("toggling mute");
                        let muted = self.play.is_muted();
                        debug!("got muted {:?}", muted);
                        self.play.set_mute(!muted);
                    }

                    PlaybackMessage::ToggleShuffle { span } => {
                        let _e = span.enter();
                        debug!("toggling shuffle");
                        todo!()
                    }

                    PlaybackMessage::Next { span } => {
                        let _e = span.enter();
                        debug!("nexting");
                        let track = {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.next_track()
                        };
                        debug!("released queue lock and got track {:?}", track);

                        self.play_or_stop(track).in_current_span().await;
                    }

                    PlaybackMessage::Prev { span } => {
                        let _e = span.enter();
                        debug!("preving");
                        let track = {
                            let mut queue = self.queue.lock().unwrap();
                            debug!("got queue lock");
                            queue.prev_track()
                        };
                        debug!("released queue lock and got track {:?}", track);
                        self.play_or_stop(track).in_current_span().await;
                    }

                    PlaybackMessage::StateChanged { state, span } => {
                        let _e = span.enter();
                        debug!("state changed");

                        let play_state = {
                            *self.state.lock().unwrap() = state.clone();
                            debug!("got state lock");

                            match state {
                                GstPlaystate::Playing => PlayState::Playing,
                                GstPlaystate::Paused => PlayState::Paused,
                                GstPlaystate::Stopped => PlayState::Stopped,
                                GstPlaystate::Buffering => PlayState::Loading,
                                _ => PlayState::Unspecified,
                            }
                        };
                        debug!("released state lock and got play state {:?}", play_state);
                        let active_track_tx = self.update_tx.clone();
                        let update = StreamUpdate::PlayState(play_state as i32);
                        if let Err(err) = active_track_tx.send(update) {
                            error!("{:?}", err)
                        };
                    }

                    PlaybackMessage::RestartTrack { span } => {
                        let _e = span.enter();
                        debug!("restarting track");
                        self.play.stop();
                        self.play.play();
                    }

                    PlaybackMessage::VolumeChanged { volume, span } => {
                        let _e = span.enter();
                        trace!("volume changed");
                        let update_tx = self.update_tx.clone();
                        let update = StreamUpdate::Volume(volume);
                        if let Err(err) = update_tx.send(update) {
                            error!("{:?}", err)
                        }
                    }

                    PlaybackMessage::MuteChanged { muted, span } => {
                        let _e = span.enter();
                        trace!("mute changed");
                        let update_tx = self.update_tx.clone();
                        let update = StreamUpdate::Mute(muted);
                        if let Err(err) = update_tx.send(update) {
                            error!("{:?}", err)
                        }
                    }

                    PlaybackMessage::PostitionChanged { position, span } => {
                        let _e = span.enter();
                        trace!("position changed");
                        let update_tx = self.update_tx.clone();
                        let duration = self
                            .play
                            .duration()
                            .and_then(|t| Some(t.mseconds() as u32))
                            .unwrap_or(0);
                        let update = StreamUpdate::Position(TrackPosition { duration, position });
                        if let Err(err) = update_tx.send(update) {
                            error!("{:?}", err)
                        }
                    }
                }
            }
        });
    }

    #[instrument(skip(self))]
    async fn flatten_node(&self, uuid: &str) -> Vec<Track> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let span = debug_span!("prov-chan");
        let Ok(_) = tx.send_async(ProviderMessage::FlattenNode {
            uuid: uuid.to_string(),
            result_tx,
            span,
        }).in_current_span().await else {
            return Vec::new();
        };
        let Ok(tracks) = result_rx
            .recv_async()
            .in_current_span()
            .await else {
                return Vec::new();
            };
        tracks
    }

    #[instrument(skip(self))]
    async fn get_track(&self, uuid: &str) -> Result<Track, ProviderError> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let span = tracing::trace_span!("prov-chan");
        tx.send_async(ProviderMessage::GetTrack {
            uuid: uuid.to_string(),
            result_tx,
            span,
        })
        .in_current_span()
        .await
        .map_err(|_| ProviderError::InternalError)?;
        result_rx
            .recv_async()
            .in_current_span()
            .await
            .map_err(|_| ProviderError::InternalError)?
    }

    #[instrument(skip(self))]
    async fn get_urls_for_track(&self, uuid: &str) -> Result<Vec<String>, ProviderError> {
        let tx = self.provider_tx.clone();
        let (result_tx, result_rx) = flume::bounded(1);
        let span = tracing::trace_span!("prov-chan");
        tx.send_async(ProviderMessage::GetTrackUrls {
            uuid: uuid.to_string(),
            result_tx,
            span,
        })
        .in_current_span()
        .await
        .map_err(|_| ProviderError::InternalError)?;
        result_rx
            .recv_async()
            .in_current_span()
            .await
            .map_err(|_| ProviderError::InternalError)?
    }

    #[instrument(skip(self))]
    async fn play_or_stop(&self, track: Option<Track>) {
        if let Some(track) = track {
            let mut uuid = track.uuid.clone();
            let urls = loop {
                match self.get_urls_for_track(&uuid).in_current_span().await {
                    Ok(urls) => break urls,
                    Err(err) => {
                        warn!("no urls found for track {:?}: {}", track.uuid, err);
                        uuid = {
                            let mut queue = self.queue.lock().unwrap();
                            if let Some(track) = queue.next_track() {
                                track.uuid.clone()
                            } else {
                                return;
                            }
                        }
                    }
                }
            };
            {
                let queue = self.queue.lock().unwrap();
                let queue_update_tx = self.update_tx.clone();
                let track = queue.current_track();
                let update = StreamUpdate::QueueTrack(QueueTrack {
                    queue_position: queue.current_position,
                    track,
                });
                if let Err(err) = queue_update_tx.send(update) {
                    error!("{:?}", err)
                }
            }
            self.play.stop();
            self.play.set_uri(Some(&urls[0]));
            self.play.play();
        } else {
            self.play.stop();
        }
    }

    #[instrument(skip(self))]
    async fn play(&self, track: Option<Track>) {
        if let Some(track) = track {
            let mut uuid = track.uuid.clone();
            let urls = loop {
                match self.get_urls_for_track(&uuid).in_current_span().await {
                    Ok(urls) => break urls,
                    Err(err) => {
                        warn!("no urls found for track {:?}: {}", track.uuid, err);
                        uuid = {
                            let mut queue = self.queue.lock().unwrap();
                            if let Some(track) = queue.next_track() {
                                track.uuid.clone()
                            } else {
                                return;
                            }
                        }
                    }
                }
            };
            {
                let queue = self.queue.lock().unwrap();
                let queue_update_tx = self.update_tx.clone();
                let track = queue.current_track();
                let update = StreamUpdate::QueueTrack(QueueTrack {
                    queue_position: queue.current_position,
                    track,
                });
                if let Err(err) = queue_update_tx.send(update) {
                    error!("{:?}", err)
                }
            }
            self.play.stop();
            self.play.set_uri(Some(&urls[0]));
            self.play.play();
        }
    }
}

fn is_track(uuid: &str) -> bool {
    uuid.starts_with("track:")
}
