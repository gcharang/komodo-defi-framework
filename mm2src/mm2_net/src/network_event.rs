use common::executor::Timer;
use mm2_core::mm_ctx::MmArc;
use mm2_event_stream::Event;
use mm2_libp2p::atomicdex_behaviour;
use serde_json::json;

use crate::p2p::P2PContext;

// TODO: Create Event trait to enforce same design for all events.

pub const NETWORK_EVENT_TYPE: &str = "NETWORK";

pub async fn start_network_event_stream(ctx: MmArc, event_interval: f64) {
    let p2p_ctx = P2PContext::fetch_from_mm_arc(&ctx);

    loop {
        let p2p_cmd_tx = p2p_ctx.cmd_tx.lock().clone();

        let peers_info = atomicdex_behaviour::get_peers_info(p2p_cmd_tx.clone()).await;
        let gossip_mesh = atomicdex_behaviour::get_gossip_mesh(p2p_cmd_tx.clone()).await;
        let gossip_peer_topics = atomicdex_behaviour::get_gossip_peer_topics(p2p_cmd_tx.clone()).await;
        let gossip_topic_peers = atomicdex_behaviour::get_gossip_topic_peers(p2p_cmd_tx.clone()).await;
        let relay_mesh = atomicdex_behaviour::get_relay_mesh(p2p_cmd_tx).await;

        let event_data = json!({
            "peers_info": peers_info,
            "gossip_mesh": gossip_mesh,
            "gossip_peer_topics": gossip_peer_topics,
            "gossip_topic_peers": gossip_topic_peers,
            "relay_mesh": relay_mesh,
        });

        ctx.stream_channel_controller
            .broadcast(Event::new(NETWORK_EVENT_TYPE.to_string(), event_data.to_string()))
            .await;

        Timer::sleep(event_interval).await;
    }
}
