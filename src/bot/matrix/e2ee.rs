use matrix_sdk::{
    self,
    config::SyncSettings,
    encryption::verification::{format_emojis, SasVerification, Verification},
    ruma::{
        events::{
            key::verification::{
                done::{OriginalSyncKeyVerificationDoneEvent, ToDeviceKeyVerificationDoneEvent},
                key::{OriginalSyncKeyVerificationKeyEvent, ToDeviceKeyVerificationKeyEvent},
                request::ToDeviceKeyVerificationRequestEvent,
                start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
            },
            room::message::{MessageType, OriginalSyncRoomMessageEvent},
        },
        UserId,
    },
    Client,
};

mod web_server {
    use std::{
        net::{SocketAddr, TcpListener},
        sync::OnceLock,
    };

    use axum::{extract::Path, http::StatusCode, routing::get, Router};
    use tokio::sync::broadcast;

    pub static TX: OnceLock<broadcast::Sender<String>> = OnceLock::new();
    pub static PORT: OnceLock<u16> = OnceLock::new();

    pub async fn axum_verify_server() {
        TX.get_or_init(|| {
            let (tx, _) = broadcast::channel(64);
            tx
        });

        let port = PORT.get_or_init(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        });

        let app: Router = Router::new().route("/verify/:code", get(verify_code));
        let addr = SocketAddr::from(([127, 0, 0, 1], *port));
        let server = axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(async move {
                tokio::signal::ctrl_c()
                    .await
                    .expect("failed to install CTRL+C handler")
            });

        server.await.unwrap();
    }

    async fn verify_code(Path(code): Path<String>) -> StatusCode {
        let result = TX.get().unwrap().send(code);

        if result.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }

        StatusCode::OK
    }
}

async fn wait_for_confirmation(client: Client, sas: SasVerification) {
    let emoji = sas.emoji().expect("The emoji should be available now");

    println!("\nDo the emojis match: \n{}", format_emojis(emoji));

    let code = uuid::Uuid::new_v4().simple().to_string();

    println!("Please run the command to allow:");
    println!(
        "local: wget  -O - http://127.0.0.1:{}/verify/{}",
        web_server::PORT.get().unwrap(),
        code
    );

    println!(
        "docker: docker exec matrix_webhook wget -O - http://127.0.0.1:{}/verify/{}",
        web_server::PORT.get().unwrap(),
        code
    );

    // 最多等待 5 分钟
    let timeout = std::time::Instant::now();
    let mut rx = web_server::TX
        .get_or_init(|| {
            let (tx, _) = tokio::sync::broadcast::channel(64);
            tx
        })
        .subscribe();

    while timeout.elapsed().as_secs() < 5 * 60 {
        if let Ok(msg) = rx.try_recv() {
            log::warn!("msg: {}", msg);
            if msg == code {
                println!("Code matches");
                sas.confirm().await.unwrap();

                if sas.is_done() {
                    print_result(&sas);
                    print_devices(sas.other_device().user_id(), &client).await;
                }
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn print_result(sas: &SasVerification) {
    let device = sas.other_device();

    println!(
        "Successfully verified device {} {} {:?}",
        device.user_id(),
        device.device_id(),
        device.local_trust_state()
    );
}

async fn print_devices(user_id: &UserId, client: &Client) {
    println!("Devices of user {}", user_id);

    for device in client
        .encryption()
        .get_user_devices(user_id)
        .await
        .unwrap()
        .devices()
    {
        println!(
            "   {:<10} {:<30} {:<}",
            device.device_id(),
            device.display_name().unwrap_or("-"),
            device.is_verified()
        );
    }
}

pub async fn sync(client: Client) -> matrix_sdk::Result<()> {
    client.add_event_handler(
        |ev: ToDeviceKeyVerificationRequestEvent, client: Client| async move {
            let request = client
                .encryption()
                .get_verification_request(&ev.sender, &ev.content.transaction_id)
                .await
                .expect("Request object wasn't created");

            request
                .accept()
                .await
                .expect("Can't accept verification request");
        },
    );

    client.add_event_handler(
        |ev: ToDeviceKeyVerificationStartEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                .await
            {
                println!(
                    "Starting verification with {} {}",
                    &sas.other_device().user_id(),
                    &sas.other_device().device_id()
                );
                print_devices(&ev.sender, &client).await;
                sas.accept().await.unwrap();
            }
        },
    );

    client.add_event_handler(
        |ev: ToDeviceKeyVerificationKeyEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                .await
            {
                tokio::spawn(wait_for_confirmation(client, sas));
            }
        },
    );

    client.add_event_handler(
        |ev: ToDeviceKeyVerificationDoneEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                .await
            {
                if sas.is_done() {
                    print_result(&sas);
                    print_devices(&ev.sender, &client).await;
                }
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncRoomMessageEvent, client: Client| async move {
            if let MessageType::VerificationRequest(_) = &ev.content.msgtype {
                let request = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.event_id)
                    .await
                    .expect("Request object wasn't created");

                request
                    .accept()
                    .await
                    .expect("Can't accept verification request");
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncKeyVerificationStartEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                .await
            {
                println!(
                    "Starting verification with {} {}",
                    &sas.other_device().user_id(),
                    &sas.other_device().device_id()
                );
                print_devices(&ev.sender, &client).await;
                sas.accept().await.unwrap();
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncKeyVerificationKeyEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                .await
            {
                tokio::spawn(wait_for_confirmation(client.clone(), sas));
            }
        },
    );

    client.add_event_handler(
        |ev: OriginalSyncKeyVerificationDoneEvent, client: Client| async move {
            if let Some(Verification::SasV1(sas)) = client
                .encryption()
                .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                .await
            {
                if sas.is_done() {
                    print_result(&sas);
                    print_devices(&ev.sender, &client).await;
                }
            }
        },
    );

    tokio::spawn(web_server::axum_verify_server());

    client.sync(SyncSettings::new()).await?;

    Ok(())
}
