use std::time::Duration;

#[tokio::main]
pub async fn main(){
    let mut battery_client = batteread::BatteryClient::new_default_name().await.unwrap();
    loop {
        let battery_state = battery_client.fetch_state().await.unwrap();
        println!("{battery_state:?}");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}