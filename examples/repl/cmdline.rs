use std::path::Path;
use std::str::FromStr;

use deltachat::chat::{self, Chat, ChatId, ChatVisibility};
use deltachat::chatlist::*;
use deltachat::constants::*;
use deltachat::contact::*;
use deltachat::context::*;
use deltachat::dc_receive_imf::*;
use deltachat::dc_tools::*;
use deltachat::error::Error;
use deltachat::imex::*;
use deltachat::job::*;
use deltachat::location;
use deltachat::lot::LotState;
use deltachat::message::{self, Message, MessageState, MsgId};
use deltachat::peerstate::*;
use deltachat::qr::*;
use deltachat::sql;
use deltachat::coi::CoiMessageFilter;
use deltachat::Event;
use deltachat::{config, provider};

/// Reset database tables.
/// Argument is a bitmask, executing single or multiple actions in one call.
/// e.g. bitmask 7 triggers actions definded with bits 1, 2 and 4.
fn dc_reset_tables(context: &Context, bits: i32) -> i32 {
    println!("Resetting tables ({})...", bits);
    if 0 != bits & 1 {
        sql::execute(context, &context.sql, "DELETE FROM jobs;", params![]).unwrap();
        println!("(1) Jobs reset.");
    }
    if 0 != bits & 2 {
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM acpeerstates;",
            params![],
        )
        .unwrap();
        println!("(2) Peerstates reset.");
    }
    if 0 != bits & 4 {
        sql::execute(context, &context.sql, "DELETE FROM keypairs;", params![]).unwrap();
        println!("(4) Private keypairs reset.");
    }
    if 0 != bits & 8 {
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM contacts WHERE id>9;",
            params![],
        )
        .unwrap();
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM chats WHERE id>9;",
            params![],
        )
        .unwrap();
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM chats_contacts;",
            params![],
        )
        .unwrap();
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM msgs WHERE id>9;",
            params![],
        )
        .unwrap();
        sql::execute(
            context,
            &context.sql,
            "DELETE FROM config WHERE keyname LIKE 'imap.%' OR keyname LIKE 'configured%';",
            params![],
        )
        .unwrap();
        sql::execute(context, &context.sql, "DELETE FROM leftgrps;", params![]).unwrap();
        println!("(8) Rest but server config reset.");
    }

    context.call_cb(Event::MsgsChanged {
        chat_id: ChatId::new(0),
        msg_id: MsgId::new(0),
    });

    1
}

fn dc_poke_eml_file(context: &Context, filename: impl AsRef<Path>) -> Result<(), Error> {
    let data = dc_read_file(context, filename)?;

    if let Err(err) = dc_receive_imf(context, &data, "import", 0, false) {
        println!("dc_receive_imf errored: {:?}", err);
    }
    Ok(())
}

/// Import a file to the database.
/// For testing, import a folder with eml-files, a single eml-file, e-mail plus public key and so on.
/// For normal importing, use imex().
///
/// @private @memberof Context
/// @param context The context as created by dc_context_new().
/// @param spec The file or directory to import. NULL for the last command.
/// @return 1=success, 0=error.
fn poke_spec(context: &Context, spec: Option<&str>) -> libc::c_int {
    if !context.sql.is_open() {
        error!(context, "Import: Database not opened.");
        return 0;
    }

    let mut read_cnt = 0;

    let real_spec: String;

    /* if `spec` is given, remember it for later usage; if it is not given, try to use the last one */
    if let Some(spec) = spec {
        real_spec = spec.to_string();
        context
            .sql
            .set_raw_config(context, "import_spec", Some(&real_spec))
            .unwrap();
    } else {
        let rs = context.sql.get_raw_config(context, "import_spec");
        if rs.is_none() {
            error!(context, "Import: No file or folder given.");
            return 0;
        }
        real_spec = rs.unwrap();
    }
    if let Some(suffix) = dc_get_filesuffix_lc(&real_spec) {
        if suffix == "eml" && dc_poke_eml_file(context, &real_spec).is_ok() {
            read_cnt += 1
        }
    } else {
        /* import a directory */
        let dir_name = std::path::Path::new(&real_spec);
        let dir = std::fs::read_dir(dir_name);
        if dir.is_err() {
            error!(context, "Import: Cannot open directory \"{}\".", &real_spec,);
            return 0;
        } else {
            let dir = dir.unwrap();
            for entry in dir {
                if entry.is_err() {
                    break;
                }
                let entry = entry.unwrap();
                let name_f = entry.file_name();
                let name = name_f.to_string_lossy();
                if name.ends_with(".eml") {
                    let path_plus_name = format!("{}/{}", &real_spec, name);
                    println!("Import: {}", path_plus_name);
                    if dc_poke_eml_file(context, path_plus_name).is_ok() {
                        read_cnt += 1
                    }
                }
            }
        }
    }
    println!("Import: {} items read from \"{}\".", read_cnt, &real_spec);
    if read_cnt > 0 {
        context.call_cb(Event::MsgsChanged {
            chat_id: ChatId::new(0),
            msg_id: MsgId::new(0),
        });
    }
    1
}

fn log_msg(context: &Context, prefix: impl AsRef<str>, msg: &Message) {
    let contact = Contact::get_by_id(context, msg.get_from_id()).expect("invalid contact");
    let contact_name = contact.get_name();
    let contact_id = contact.get_id();

    let statestr = match msg.get_state() {
        MessageState::OutPending => " o",
        MessageState::OutDelivered => " √",
        MessageState::OutMdnRcvd => " √√",
        MessageState::OutFailed => " !!",
        _ => "",
    };
    let temp2 = dc_timestamp_to_str(msg.get_timestamp());
    let msgtext = msg.get_text();
    println!(
        "{}{}{}{}: {} (Contact#{}): {} {}{}{}{}{} [{}]",
        prefix.as_ref(),
        msg.get_id(),
        if msg.get_showpadlock() { "🔒" } else { "" },
        if msg.has_location() { "📍" } else { "" },
        &contact_name,
        contact_id,
        msgtext.unwrap_or_default(),
        if msg.is_starred() { "★" } else { "" },
        if msg.get_from_id() == 1 as libc::c_uint {
            ""
        } else if msg.get_state() == MessageState::InSeen {
            "[SEEN]"
        } else if msg.get_state() == MessageState::InNoticed {
            "[NOTICED]"
        } else {
            "[FRESH]"
        },
        if msg.is_info() { "[INFO]" } else { "" },
        if msg.is_forwarded() {
            "[FORWARDED]"
        } else {
            ""
        },
        statestr,
        &temp2,
    );
}

fn log_msglist(context: &Context, msglist: &Vec<MsgId>) -> Result<(), Error> {
    let mut lines_out = 0;
    for &msg_id in msglist {
        if msg_id.is_daymarker() {
            println!(
                "--------------------------------------------------------------------------------"
            );

            lines_out += 1
        } else if !msg_id.is_special() {
            if lines_out == 0 {
                println!(
                    "--------------------------------------------------------------------------------",
                );
                lines_out += 1
            }
            let msg = Message::load_from_db(context, msg_id)?;
            log_msg(context, "", &msg);
        }
    }
    if lines_out > 0 {
        println!(
            "--------------------------------------------------------------------------------"
        );
    }
    Ok(())
}

fn log_contactlist(context: &Context, contacts: &Vec<u32>) {
    let mut contacts = contacts.clone();
    if !contacts.contains(&1) {
        contacts.push(1);
    }
    for contact_id in contacts {
        let line;
        let mut line2 = "".to_string();
        if let Ok(contact) = Contact::get_by_id(context, contact_id) {
            let name = contact.get_name();
            let addr = contact.get_addr();
            let verified_state = contact.is_verified(context);
            let verified_str = if VerifiedStatus::Unverified != verified_state {
                if verified_state == VerifiedStatus::BidirectVerified {
                    " √√"
                } else {
                    " √"
                }
            } else {
                ""
            };
            line = format!(
                "{}{} <{}>",
                if !name.is_empty() {
                    &name
                } else {
                    "<name unset>"
                },
                verified_str,
                if !addr.is_empty() {
                    &addr
                } else {
                    "addr unset"
                }
            );
            let peerstate = Peerstate::from_addr(context, &context.sql, &addr);
            if peerstate.is_some() && contact_id != 1 as libc::c_uint {
                line2 = format!(
                    ", prefer-encrypt={}",
                    peerstate.as_ref().unwrap().prefer_encrypt
                );
            }

            println!("Contact#{}: {}{}", contact_id, line, line2);
        }
    }
}

fn chat_prefix(chat: &Chat) -> &'static str {
    chat.typ.into()
}

pub fn dc_cmdline(context: &Context, line: &str) -> Result<(), failure::Error> {
    let chat_id = *context.cmdline_sel_chat_id.read().unwrap();
    let mut sel_chat = if !chat_id.is_unset() {
        Chat::load_from_db(context, chat_id).ok()
    } else {
        None
    };

    let mut args = line.splitn(3, ' ');
    let arg0 = args.next().unwrap_or_default();
    let arg1 = args.next().unwrap_or_default();
    let arg2 = args.next().unwrap_or_default();

    let blobdir = context.get_blobdir();
    match arg0 {
        "help" | "?" => match arg1 {
            // TODO: reuse commands definition in main.rs.
            "imex" => println!(
                "====================Import/Export commands==\n\
                 initiate-key-transfer\n\
                 get-setupcodebegin <msg-id>\n\
                 continue-key-transfer <msg-id> <setup-code>\n\
                 has-backup\n\
                 export-backup\n\
                 import-backup <backup-file>\n\
                 export-keys\n\
                 import-keys\n\
                 export-setup\n\
                 poke [<eml-file>|<folder>|<addr> <key-file>]\n\
                 reset <flags>\n\
                 stop\n\
                 ============================================="
            ),
            _ => println!(
                "==========================Database commands==\n\
                 info\n\
                 open <file to open or create>\n\
                 close\n\
                 set <configuration-key> [<value>]\n\
                 get <configuration-key>\n\
                 oauth2\n\
                 configure\n\
                 connect\n\
                 disconnect\n\
                 interrupt\n\
                 maybenetwork\n\
                 housekeeping\n\
                 help imex (Import/Export)\n\
                 ==============================Chat commands==\n\
                 listchats [<query>]\n\
                 listarchived\n\
                 chat [<chat-id>|0]\n\
                 createchat <contact-id>\n\
                 createchatbymsg <msg-id>\n\
                 creategroup <name>\n\
                 createverified <name>\n\
                 addmember <contact-id>\n\
                 removemember <contact-id>\n\
                 groupname <name>\n\
                 groupimage [<file>]\n\
                 chatinfo\n\
                 sendlocations <seconds>\n\
                 setlocation <lat> <lng>\n\
                 dellocations\n\
                 getlocations [<contact-id>]\n\
                 send <text>\n\
                 send-garbage\n\
                 sendimage <file> [<text>]\n\
                 sendfile <file> [<text>]\n\
                 draft [<text>]\n\
                 devicemsg <text>\n\
                 listmedia\n\
                 archive <chat-id>\n\
                 unarchive <chat-id>\n\
                 pin <chat-id>\n\
                 unpin <chat-id>\n\
                 delchat <chat-id>\n\
                 ===========================Message commands==\n\
                 listmsgs <query>\n\
                 msginfo <msg-id>\n\
                 listfresh\n\
                 forward <msg-id> <chat-id>\n\
                 markseen <msg-id>\n\
                 star <msg-id>\n\
                 unstar <msg-id>\n\
                 delmsg <msg-id>\n\
                 ===========================Contact commands==\n\
                 listcontacts [<query>]\n\
                 listverified [<query>]\n\
                 addcontact [<name>] <addr>\n\
                 contactinfo <contact-id>\n\
                 delcontact <contact-id>\n\
                 cleanupcontacts\n\
                 ======================================Coi====\n\
                 coi-enable\n\
                 coi-disable\n\
                 coi-set-message-filter [none | active | seen]\n\
                 coi-get-message-filter\n\
                 ======================================Misc.==\n\
                 getqr [<chat-id>]\n\
                 getbadqr\n\
                 checkqr <qr-content>\n\
                 providerinfo <addr>\n\
                 event <event-id to test>\n\
                 fileinfo <file>\n\
                 emptyserver <flags> (1=MVBOX 2=INBOX)\n\
                 clear -- clear screen\n\
                 exit or quit\n\
                 ============================================="
            ),
        },
        "initiate-key-transfer" => match initiate_key_transfer(context) {
            Ok(setup_code) => println!(
                "Setup code for the transferred setup message: {}",
                setup_code,
            ),
            Err(err) => bail!("Failed to generate setup code: {}", err),
        },
        "get-setupcodebegin" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing.");
            let msg_id: MsgId = MsgId::new(arg1.parse()?);
            let msg = Message::load_from_db(context, msg_id)?;
            if msg.is_setupmessage() {
                let setupcodebegin = msg.get_setupcodebegin(context);
                println!(
                    "The setup code for setup message {} starts with: {}",
                    msg_id,
                    setupcodebegin.unwrap_or_default(),
                );
            } else {
                bail!("{} is no setup message.", msg_id,);
            }
        }
        "continue-key-transfer" => {
            ensure!(
                !arg1.is_empty() && !arg2.is_empty(),
                "Arguments <msg-id> <setup-code> expected"
            );
            continue_key_transfer(context, MsgId::new(arg1.parse()?), &arg2)?;
        }
        "has-backup" => {
            has_backup(context, blobdir)?;
        }
        "export-backup" => {
            imex(context, ImexMode::ExportBackup, Some(blobdir));
        }
        "import-backup" => {
            ensure!(!arg1.is_empty(), "Argument <backup-file> missing.");
            imex(context, ImexMode::ImportBackup, Some(arg1));
        }
        "export-keys" => {
            imex(context, ImexMode::ExportSelfKeys, Some(blobdir));
        }
        "import-keys" => {
            imex(context, ImexMode::ImportSelfKeys, Some(blobdir));
        }
        "export-setup" => {
            let setup_code = create_setup_code(context);
            let file_name = blobdir.join("autocrypt-setup-message.html");
            let file_content = render_setup_file(context, &setup_code)?;
            std::fs::write(&file_name, file_content)?;
            println!(
                "Setup message written to: {}\nSetup code: {}",
                file_name.display(),
                &setup_code,
            );
        }
        "poke" => {
            ensure!(0 != poke_spec(context, Some(arg1)), "Poke failed");
        }
        "reset" => {
            ensure!(!arg1.is_empty(), "Argument <bits> missing: 1=jobs, 2=peerstates, 4=private keys, 8=rest but server config");
            let bits: i32 = arg1.parse()?;
            ensure!(bits < 16, "<bits> must be lower than 16.");
            ensure!(0 != dc_reset_tables(context, bits), "Reset failed");
        }
        "stop" => {
            context.stop_ongoing();
        }
        "set" => {
            ensure!(!arg1.is_empty(), "Argument <key> missing.");
            let key = config::Config::from_str(&arg1)?;
            let value = if arg2.is_empty() { None } else { Some(arg2) };
            context.set_config(key, value)?;
        }
        "get" => {
            ensure!(!arg1.is_empty(), "Argument <key> missing.");
            let key = config::Config::from_str(&arg1)?;
            let val = context.get_config(key);
            println!("{}={:?}", key, val);
        }
        "info" => {
            println!("{:#?}", context.get_info());
        }
        "interrupt" => {
            interrupt_inbox_idle(context);
        }
        "maybenetwork" => {
            maybe_network(context);
        }
        "housekeeping" => {
            sql::housekeeping(context);
        }
        "listchats" | "listarchived" | "chats" => {
            let listflags = if arg0 == "listarchived" { 0x01 } else { 0 };
            let chatlist = Chatlist::try_load(
                context,
                listflags,
                if arg1.is_empty() { None } else { Some(arg1) },
                None,
            )?;

            let cnt = chatlist.len();
            if cnt > 0 {
                println!(
                    "================================================================================"
                );

                for i in (0..cnt).rev() {
                    let chat = Chat::load_from_db(context, chatlist.get_chat_id(i))?;
                    println!(
                        "{}#{}: {} [{} fresh] {}",
                        chat_prefix(&chat),
                        chat.get_id(),
                        chat.get_name(),
                        chat.get_id().get_fresh_msg_cnt(context),
                        match chat.visibility {
                            ChatVisibility::Normal => "",
                            ChatVisibility::Archived => "📦",
                            ChatVisibility::Pinned => "📌",
                        },
                    );
                    let lot = chatlist.get_summary(context, i, Some(&chat));
                    let statestr = if chat.visibility == ChatVisibility::Archived {
                        " [Archived]"
                    } else {
                        match lot.get_state() {
                            LotState::MsgOutPending => " o",
                            LotState::MsgOutDelivered => " √",
                            LotState::MsgOutMdnRcvd => " √√",
                            LotState::MsgOutFailed => " !!",
                            _ => "",
                        }
                    };
                    let timestr = dc_timestamp_to_str(lot.get_timestamp());
                    let text1 = lot.get_text1();
                    let text2 = lot.get_text2();
                    println!(
                        "{}{}{}{} [{}]{}",
                        text1.unwrap_or(""),
                        if text1.is_some() { ": " } else { "" },
                        text2.unwrap_or(""),
                        statestr,
                        &timestr,
                        if chat.is_sending_locations() {
                            "📍"
                        } else {
                            ""
                        },
                    );
                    println!(
                        "================================================================================"
                    );
                }
            }
            if location::is_sending_locations_to_chat(context, ChatId::new(0)) {
                println!("Location streaming enabled.");
            }
            println!("{} chats", cnt);
        }
        "chat" => {
            if sel_chat.is_none() && arg1.is_empty() {
                bail!("Argument [chat-id] is missing.");
            }
            if !arg1.is_empty() {
                let chat_id = ChatId::new(arg1.parse()?);
                println!("Selecting chat {}", chat_id);
                sel_chat = Some(Chat::load_from_db(context, chat_id)?);
                *context.cmdline_sel_chat_id.write().unwrap() = chat_id;
            }

            ensure!(sel_chat.is_some(), "Failed to select chat");
            let sel_chat = sel_chat.as_ref().unwrap();

            let msglist = chat::get_chat_msgs(context, sel_chat.get_id(), 0x1, None);
            let members = chat::get_chat_contacts(context, sel_chat.id);
            let subtitle = if sel_chat.is_device_talk() {
                "device-talk".to_string()
            } else if sel_chat.get_type() == Chattype::Single && !members.is_empty() {
                let contact = Contact::get_by_id(context, members[0])?;
                contact.get_addr().to_string()
            } else {
                format!("{} member(s)", members.len())
            };
            println!(
                "{}#{}: {} [{}]{}{}",
                chat_prefix(sel_chat),
                sel_chat.get_id(),
                sel_chat.get_name(),
                subtitle,
                if sel_chat.is_sending_locations() {
                    "📍"
                } else {
                    ""
                },
                match sel_chat.get_profile_image(context) {
                    Some(icon) => match icon.to_str() {
                        Some(icon) => format!(" Icon: {}", icon),
                        _ => " Icon: Err".to_string(),
                    },
                    _ => "".to_string(),
                },
            );
            log_msglist(context, &msglist)?;
            if let Some(draft) = sel_chat.get_id().get_draft(context)? {
                log_msg(context, "Draft", &draft);
            }

            println!("{} messages.", sel_chat.get_id().get_msg_cnt(context));
            chat::marknoticed_chat(context, sel_chat.get_id())?;
        }
        "createchat" => {
            ensure!(!arg1.is_empty(), "Argument <contact-id> missing.");
            let contact_id: libc::c_int = arg1.parse()?;
            let chat_id = chat::create_by_contact_id(context, contact_id as u32)?;

            println!("Single#{} created successfully.", chat_id,);
        }
        "createchatbymsg" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing");
            let msg_id = MsgId::new(arg1.parse()?);
            let chat_id = chat::create_by_msg_id(context, msg_id)?;
            let chat = Chat::load_from_db(context, chat_id)?;

            println!("{}#{} created successfully.", chat_prefix(&chat), chat_id,);
        }
        "creategroup" => {
            ensure!(!arg1.is_empty(), "Argument <name> missing.");
            let chat_id = chat::create_group_chat(context, VerifiedStatus::Unverified, arg1)?;

            println!("Group#{} created successfully.", chat_id);
        }
        "createverified" => {
            ensure!(!arg1.is_empty(), "Argument <name> missing.");
            let chat_id = chat::create_group_chat(context, VerifiedStatus::Verified, arg1)?;

            println!("VerifiedGroup#{} created successfully.", chat_id);
        }
        "addmember" => {
            ensure!(sel_chat.is_some(), "No chat selected");
            ensure!(!arg1.is_empty(), "Argument <contact-id> missing.");

            let contact_id_0: libc::c_int = arg1.parse()?;
            if chat::add_contact_to_chat(
                context,
                sel_chat.as_ref().unwrap().get_id(),
                contact_id_0 as u32,
            ) {
                println!("Contact added to chat.");
            } else {
                bail!("Cannot add contact to chat.");
            }
        }
        "removemember" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "Argument <contact-id> missing.");
            let contact_id_1: libc::c_int = arg1.parse()?;
            chat::remove_contact_from_chat(
                context,
                sel_chat.as_ref().unwrap().get_id(),
                contact_id_1 as u32,
            )?;

            println!("Contact added to chat.");
        }
        "groupname" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "Argument <name> missing.");
            chat::set_chat_name(context, sel_chat.as_ref().unwrap().get_id(), arg1)?;

            println!("Chat name set");
        }
        "groupimage" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "Argument <image> missing.");

            chat::set_chat_profile_image(context, sel_chat.as_ref().unwrap().get_id(), arg1)?;

            println!("Chat image set");
        }
        "chatinfo" => {
            ensure!(sel_chat.is_some(), "No chat selected.");

            let contacts = chat::get_chat_contacts(context, sel_chat.as_ref().unwrap().get_id());
            println!("Memberlist:");

            log_contactlist(context, &contacts);
            println!(
                "{} contacts\nLocation streaming: {}",
                contacts.len(),
                location::is_sending_locations_to_chat(
                    context,
                    sel_chat.as_ref().unwrap().get_id()
                ),
            );
        }
        "getlocations" => {
            ensure!(sel_chat.is_some(), "No chat selected.");

            let contact_id = arg1.parse().unwrap_or_default();
            let locations = location::get_range(
                context,
                sel_chat.as_ref().unwrap().get_id(),
                contact_id,
                0,
                0,
            );
            let default_marker = "-".to_string();
            for location in &locations {
                let marker = location.marker.as_ref().unwrap_or(&default_marker);
                println!(
                    "Loc#{}: {}: lat={} lng={} acc={} Chat#{} Contact#{} {} {}",
                    location.location_id,
                    dc_timestamp_to_str(location.timestamp),
                    location.latitude,
                    location.longitude,
                    location.accuracy,
                    location.chat_id,
                    location.contact_id,
                    location.msg_id,
                    marker
                );
            }
            if locations.is_empty() {
                println!("No locations.");
            }
        }
        "sendlocations" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "No timeout given.");

            let seconds = arg1.parse()?;
            location::send_locations_to_chat(context, sel_chat.as_ref().unwrap().get_id(), seconds);
            println!(
                "Locations will be sent to Chat#{} for {} seconds. Use 'setlocation <lat> <lng>' to play around.",
                sel_chat.as_ref().unwrap().get_id(),
                seconds
            );
        }
        "setlocation" => {
            ensure!(
                !arg1.is_empty() && !arg2.is_empty(),
                "Latitude or longitude not given."
            );
            let latitude = arg1.parse()?;
            let longitude = arg2.parse()?;

            let continue_streaming = location::set(context, latitude, longitude, 0.);
            if continue_streaming {
                println!("Success, streaming should be continued.");
            } else {
                println!("Success, streaming can be stoppped.");
            }
        }
        "dellocations" => {
            location::delete_all(context)?;
        }
        "send" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "No message text given.");

            let msg = format!("{} {}", arg1, arg2);

            chat::send_text_msg(context, sel_chat.as_ref().unwrap().get_id(), msg)?;
        }
        "sendempty" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            chat::send_text_msg(context, sel_chat.as_ref().unwrap().get_id(), "".into())?;
        }
        "sendimage" | "sendfile" => {
            ensure!(sel_chat.is_some(), "No chat selected.");
            ensure!(!arg1.is_empty(), "No file given.");

            let mut msg = Message::new(if arg0 == "sendimage" {
                Viewtype::Image
            } else {
                Viewtype::File
            });
            msg.set_file(arg1, None);
            if !arg2.is_empty() {
                msg.set_text(Some(arg2.to_string()));
            }
            chat::send_msg(context, sel_chat.as_ref().unwrap().get_id(), &mut msg)?;
        }
        "listmsgs" => {
            ensure!(!arg1.is_empty(), "Argument <query> missing.");

            let chat = if let Some(ref sel_chat) = sel_chat {
                sel_chat.get_id()
            } else {
                ChatId::new(0)
            };

            let msglist = context.search_msgs(chat, arg1);

            log_msglist(context, &msglist)?;
            println!("{} messages.", msglist.len());
        }
        "draft" => {
            ensure!(sel_chat.is_some(), "No chat selected.");

            if !arg1.is_empty() {
                let mut draft = Message::new(Viewtype::Text);
                draft.set_text(Some(arg1.to_string()));
                sel_chat
                    .as_ref()
                    .unwrap()
                    .get_id()
                    .set_draft(context, Some(&mut draft));
                println!("Draft saved.");
            } else {
                sel_chat.as_ref().unwrap().get_id().set_draft(context, None);
                println!("Draft deleted.");
            }
        }
        "devicemsg" => {
            ensure!(
                !arg1.is_empty(),
                "Please specify text to add as device message."
            );
            let mut msg = Message::new(Viewtype::Text);
            msg.set_text(Some(arg1.to_string()));
            chat::add_device_msg(context, None, Some(&mut msg))?;
        }
        "updatedevicechats" => {
            context.update_device_chats()?;
        }
        "listmedia" => {
            ensure!(sel_chat.is_some(), "No chat selected.");

            let images = chat::get_chat_media(
                context,
                sel_chat.as_ref().unwrap().get_id(),
                Viewtype::Image,
                Viewtype::Gif,
                Viewtype::Video,
            );
            println!("{} images or videos: ", images.len());
            for (i, data) in images.iter().enumerate() {
                if 0 == i {
                    print!("{}", data);
                } else {
                    print!(", {}", data);
                }
            }
            print!("\n");
        }
        "archive" | "unarchive" | "pin" | "unpin" => {
            ensure!(!arg1.is_empty(), "Argument <chat-id> missing.");
            let chat_id = ChatId::new(arg1.parse()?);
            chat_id.set_visibility(
                context,
                match arg0 {
                    "archive" => ChatVisibility::Archived,
                    "unarchive" | "unpin" => ChatVisibility::Normal,
                    "pin" => ChatVisibility::Pinned,
                    _ => panic!("Unexpected command (This should never happen)"),
                },
            )?;
        }
        "delchat" => {
            ensure!(!arg1.is_empty(), "Argument <chat-id> missing.");
            let chat_id = ChatId::new(arg1.parse()?);
            chat_id.delete(context)?;
        }
        "msginfo" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing.");
            let id = MsgId::new(arg1.parse()?);
            let res = message::get_msg_info(context, id);
            println!("{}", res);
        }
        "listfresh" => {
            let msglist = context.get_fresh_msgs();

            log_msglist(context, &msglist)?;
            print!("{} fresh messages.", msglist.len());
        }
        "forward" => {
            ensure!(
                !arg1.is_empty() && !arg2.is_empty(),
                "Arguments <msg-id> <chat-id> expected"
            );

            let mut msg_ids = [MsgId::new(0); 1];
            let chat_id = ChatId::new(arg2.parse()?);
            msg_ids[0] = MsgId::new(arg1.parse()?);
            chat::forward_msgs(context, &msg_ids, chat_id)?;
        }
        "markseen" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing.");
            let mut msg_ids = [MsgId::new(0); 1];
            msg_ids[0] = MsgId::new(arg1.parse()?);
            message::markseen_msgs(context, &msg_ids);
        }
        "star" | "unstar" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing.");
            let mut msg_ids = [MsgId::new(0); 1];
            msg_ids[0] = MsgId::new(arg1.parse()?);
            message::star_msgs(context, &msg_ids, arg0 == "star");
        }
        "delmsg" => {
            ensure!(!arg1.is_empty(), "Argument <msg-id> missing.");
            let mut ids = [MsgId::new(0); 1];
            ids[0] = MsgId::new(arg1.parse()?);
            message::delete_msgs(context, &ids);
        }
        "listcontacts" | "contacts" | "listverified" => {
            let contacts = Contact::get_all(
                context,
                if arg0 == "listverified" {
                    0x1 | 0x2
                } else {
                    0x2
                },
                Some(arg1),
            )?;
            log_contactlist(context, &contacts);
            println!("{} contacts.", contacts.len());
        }
        "addcontact" => {
            ensure!(!arg1.is_empty(), "Arguments [<name>] <addr> expected.");

            if !arg2.is_empty() {
                let book = format!("{}\n{}", arg1, arg2);
                Contact::add_address_book(context, book)?;
            } else {
                Contact::create(context, "", arg1)?;
            }
        }
        "contactinfo" => {
            ensure!(!arg1.is_empty(), "Argument <contact-id> missing.");

            let contact_id = arg1.parse()?;
            let contact = Contact::get_by_id(context, contact_id)?;
            let name_n_addr = contact.get_name_n_addr();

            let mut res = format!(
                "Contact info for: {}:\nIcon: {}\n",
                name_n_addr,
                match contact.get_profile_image(context) {
                    Some(image) => image.to_str().unwrap().to_string(),
                    None => "NoIcon".to_string(),
                }
            );

            res += &Contact::get_encrinfo(context, contact_id)?;

            let chatlist = Chatlist::try_load(context, 0, None, Some(contact_id))?;
            let chatlist_cnt = chatlist.len();
            if chatlist_cnt > 0 {
                res += &format!(
                    "\n\n{} chats shared with Contact#{}: ",
                    chatlist_cnt, contact_id,
                );
                for i in 0..chatlist_cnt {
                    if 0 != i {
                        res += ", ";
                    }
                    let chat = Chat::load_from_db(context, chatlist.get_chat_id(i))?;
                    res += &format!("{}#{}", chat_prefix(&chat), chat.get_id());
                }
            }

            println!("{}", res);
        }
        "delcontact" => {
            ensure!(!arg1.is_empty(), "Argument <contact-id> missing.");
            Contact::delete(context, arg1.parse()?)?;
        }
        "checkqr" => {
            ensure!(!arg1.is_empty(), "Argument <qr-content> missing.");
            let res = check_qr(context, arg1);
            println!(
                "state={}, id={}, text1={:?}, text2={:?}",
                res.get_state(),
                res.get_id(),
                res.get_text1(),
                res.get_text2()
            );
        }
        "setqr" => {
            ensure!(!arg1.is_empty(), "Argument <qr-content> missing.");
            match set_config_from_qr(context, arg1) {
                Ok(()) => println!("Config set from QR code, you can now call 'configure'"),
                Err(err) => println!("Cannot set config from QR code: {:?}", err),
            }
        }
        "providerinfo" => {
            ensure!(!arg1.is_empty(), "Argument <addr> missing.");
            match provider::get_provider_info(arg1) {
                Some(info) => {
                    println!("Information for provider belonging to {}:", arg1);
                    println!("status: {}", info.status as u32);
                    println!("before_login_hint: {}", info.before_login_hint);
                    println!("after_login_hint: {}", info.after_login_hint);
                    println!("overview_page: {}", info.overview_page);
                    for server in info.server.iter() {
                        println!("server: {}:{}", server.hostname, server.port,);
                    }
                }
                None => {
                    println!("No information for provider belonging to {} found.", arg1);
                }
            }
        }
        // TODO: implement this again, unclear how to match this through though, without writing a parser.
        // "event" => {
        //     ensure!(!arg1.is_empty(), "Argument <id> missing.");
        //     let event = arg1.parse()?;
        //     let event = Event::from_u32(event).ok_or(format_err!("Event::from_u32({})", event))?;
        //     let r = context.call_cb(event, 0 as libc::uintptr_t, 0 as libc::uintptr_t);
        //     println!(
        //         "Sending event {:?}({}), received value {}.",
        //         event, event as usize, r as libc::c_int,
        //     );
        // }
        "fileinfo" => {
            ensure!(!arg1.is_empty(), "Argument <file> missing.");

            if let Ok(buf) = dc_read_file(context, &arg1) {
                let (width, height) = dc_get_filemeta(&buf)?;
                println!("width={}, height={}", width, height);
            } else {
                bail!("Command failed.");
            }
        }
        "coi-enable" => {
            let id = 1; // XXX
            context.set_coi_enabled(true, id);
            println!("coi-enable command queued with id: {}", id);
        }
        "coi-disable" => {
            let id = 1; // XXX
            context.set_coi_enabled(false, id);
            println!("coi-disable command queued with id: {}", id);
        }
        "coi-set-message-filter" => {
            ensure!(!arg1.is_empty(), "Argument <message-filter> missing.");
            if let Ok(message_filter) = CoiMessageFilter::from_str(&arg1) {
                let id = 1; // XXX
                context.set_coi_message_filter(message_filter, id);
                println!("coi-set-message-filter command queued with id: {}", id);
            }
            else {
                bail!("Invalid message-filter argument. Requires: none, active or seen");
            }
        }
        "coi-get-message-filter" => {
            let id = 1; // XXX
            context.get_coi_message_filter(id);
            println!("coi-get-message-filter command queued with id: {}", id);
        }
        "emptyserver" => {
            ensure!(!arg1.is_empty(), "Argument <flags> missing");

            message::dc_empty_server(context, arg1.parse()?);
        }
        "" => (),
        _ => bail!("Unknown command: \"{}\" type ? for help.", arg0),

    }
    Ok(())
}
