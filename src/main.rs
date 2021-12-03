use discord::model::{Event, MessageReaction, ReactionEmoji};
use discord::{Discord, State};
use std::env;
use std::ops::RangeBounds;

fn main() {
    // Log in to discord using a bot token
    let discord = Discord::from_bot_token(&env::var("DISCORD_TOKEN").expect("Expected token"))
        .expect("Login failed");

    // establish websocket and voice connecction
    let (mut connection, ready) = discord.connect().expect("connect failed");
    println!(
        "[READY!] {} is serving {} servers",
        ready.user.username,
        ready.servers.len()
    );

    let mut state = State::new(ready);
    connection.sync_calls(&state.all_private_channels());

    // receive events forever
    loop {
        let event = match connection.recv_event() {
            Ok(event) => event,
            Err(err) => {
                println!("[Warning] Received error: {:?}", err);
                if let discord::Error::WebSocket(..) = err {
                    // Handle the websocket connection being dropped
                    let (new_connection, ready) = discord.connect().expect("connect failed");
                    connection = new_connection;
                    state = State::new(ready);
                    println!("[Ready] Reconnected successfully.");
                }
                if let discord::Error::Closed(..) = err {
                    break;
                }
                continue;
            }
        };

        state.update(&event);

        match event {
            Event::MessageCreate(message) => {
                // safeguard: stop if the message is from the bot
                if message.author.id == state.user().id {
                    continue;
                }
                // reply to a command if there was one
                let mut split = message.content.split(' ');
                let first_word = split.next().unwrap_or("");
                let argument = split.next().unwrap_or("");

                if first_word.eq_ignore_ascii_case("!h") {
                    println!("[Message Received]: {:?}", message);
                    let vchan = state.find_voice_user(message.author.id);
                    if argument.eq_ignore_ascii_case("stop") {
                        discord.add_reaction(
                            message.channel_id,
                            message.id,
                            ReactionEmoji::Unicode("🛑".to_string()),
                        );
                        vchan.map(|(sid, _)| connection.voice(sid).stop());
                    } else if argument.eq_ignore_ascii_case("quit")
                        || argument.eq_ignore_ascii_case("fuckoff")
                    {
                        discord.add_reaction(
                            message.channel_id,
                            message.id,
                            ReactionEmoji::Unicode("🏃".to_string()),
                        );
                        vchan.map(|(sid, _)| connection.drop_voice(sid));
                    } else {
                        let output = if let Some((server_id, channel_id)) = vchan {
                            match discord::voice::open_ytdl_stream(argument) {
                                Ok(stream) => {
                                    discord.add_reaction(
                                        message.channel_id,
                                        message.id,
                                        ReactionEmoji::Unicode("▶️".to_string()),
                                    );
                                    let voice = connection.voice(server_id);
                                    voice.set_deaf(true);
                                    voice.connect(channel_id);
                                    voice.play(stream);
                                    String::new()
                                }
                                Err(error) => format!("[Error]: {}", error),
                            }
                        } else {
                            "You must be in a voice channel to use that command 😉".to_owned()
                        };
                        if !output.is_empty() {
                            warn(discord.send_message(message.channel_id, &output, "", false));
                        }
                    }
                }
            }
            Event::VoiceStateUpdate(server_id, _) => {
                // If someone moves/hangs up, and we are in a voice channel,
                if let Some(cur_channel) = connection.voice(server_id).current_channel() {
                    // and our current voice channel is empty, disconnect from voice
                    match server_id {
                        Some(server_id) => {
                            if let Some(srv) =
                                state.servers().iter().find(|srv| srv.id == server_id)
                            {
                                if srv
                                    .voice_states
                                    .iter()
                                    .filter(|vs| vs.channel_id == Some(cur_channel))
                                    .count()
                                    <= 1
                                {
                                    connection.voice(Some(server_id)).disconnect();
                                }
                            }
                        }
                        None => {
                            if let Some(call) = state.calls().get(&cur_channel) {
                                if call.voice_states.len() <= 1 {
                                    connection.voice(server_id).disconnect();
                                }
                            }
                        }
                    }
                }
            }
            Event::ReactionAdd(reaction) => {
                if reaction.user_id != state.user().id
                    && reaction.emoji == ReactionEmoji::Unicode("👆".to_string())
                {
                    println!("[Received] Reaction: {:?}", reaction);
                    let output = match discord.get_message(reaction.channel_id, reaction.message_id)
                    {
                        Ok(message) => {
                            let mut url = message.content;
                            if url.contains(' ') {
                                url = url.split(' ').filter(|x| x.contains("https://")).collect();
                            }
                            let vchan = state.find_voice_user(message.author.id);
                            if let Some((server_id, channel_id)) = vchan {
                                match discord::voice::open_ytdl_stream(url.as_str()) {
                                    Ok(stream) => {
                                        discord.add_reaction(
                                            message.channel_id,
                                            message.id,
                                            ReactionEmoji::Unicode("▶️".to_string()),
                                        );
                                        let voice = connection.voice(server_id);
                                        voice.set_deaf(true);
                                        voice.connect(channel_id);
                                        voice.play(stream);
                                        String::new()
                                    }
                                    Err(error) => format!("[Error]: {}", error),
                                }
                            } else {
                                format!("[Error] connecting to voice channel")
                            }
                        }
                        Err(err) => "Error acquiring message data".to_string(),
                    };
                    if !output.is_empty() {
                        warn(discord.send_message(reaction.channel_id, &output, "", false));
                    }
                }
            }
            _ => {}
        }
    }
}

fn warn<T, E: ::std::fmt::Debug>(result: Result<T, E>) {
    match result {
        Ok(_) => {}
        Err(err) => println!("[Warning] {:?}", err),
    }
}
