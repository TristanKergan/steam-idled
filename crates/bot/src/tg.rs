use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub is_bot: Option<bool>,
    pub first_name: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    pub message: Option<Message>,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
}

impl InlineKeyboardButton {
    pub fn callback(text: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            callback_data: Some(data.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

impl InlineKeyboardMarkup {
    pub fn new(rows: Vec<Vec<InlineKeyboardButton>>) -> Self {
        Self {
            inline_keyboard: rows,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Clone)]
pub struct TgClient {
    client: Client,
    base_url: String,
}

impl TgClient {
    pub fn new(bot_token: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: format!("https://api.telegram.org/bot{}", bot_token),
        }
    }

    pub async fn get_me(&self) -> Result<User> {
        let url = format!("{}/getMe", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("getMe request failed")?;
        let tg_res: TgResponse<User> = resp.json().await.context("getMe invalid JSON")?;

        if !tg_res.ok || tg_res.result.is_none() {
            bail!("Telegram getMe error: {:?}", tg_res.description);
        }
        Ok(tg_res.result.unwrap())
    }

    pub async fn get_updates(&self, offset: Option<i64>, timeout_secs: u64) -> Result<Vec<Update>> {
        let url = format!("{}/getUpdates", self.base_url);
        let mut query = vec![("timeout", timeout_secs.to_string())];
        if let Some(off) = offset {
            query.push(("offset", off.to_string()));
        }

        let resp = self.client.get(&url).query(&query).send().await?;
        let tg_res: TgResponse<Vec<Update>> = resp.json().await?;

        if !tg_res.ok {
            bail!("Telegram getUpdates error: {:?}", tg_res.description);
        }

        Ok(tg_res.result.unwrap_or_default())
    }

    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<&InlineKeyboardMarkup>,
    ) -> Result<Message> {
        let url = format!("{}/sendMessage", self.base_url);

        #[derive(Serialize)]
        struct SendMsg<'a> {
            chat_id: i64,
            text: &'a str,
            parse_mode: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            reply_markup: Option<&'a InlineKeyboardMarkup>,
        }

        let payload = SendMsg {
            chat_id,
            text,
            parse_mode: "HTML",
            reply_markup,
        };

        let resp = self.client.post(&url).json(&payload).send().await?;
        let tg_res: TgResponse<Message> = resp.json().await?;

        if !tg_res.ok || tg_res.result.is_none() {
            bail!("Telegram sendMessage error: {:?}", tg_res.description);
        }
        Ok(tg_res.result.unwrap())
    }

    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<&InlineKeyboardMarkup>,
    ) -> Result<()> {
        let url = format!("{}/editMessageText", self.base_url);

        #[derive(Serialize)]
        struct EditMsg<'a> {
            chat_id: i64,
            message_id: i64,
            text: &'a str,
            parse_mode: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            reply_markup: Option<&'a InlineKeyboardMarkup>,
        }

        let payload = EditMsg {
            chat_id,
            message_id,
            text,
            parse_mode: "HTML",
            reply_markup,
        };

        let resp = self.client.post(&url).json(&payload).send().await?;
        let tg_res: TgResponse<serde_json::Value> = resp.json().await?;

        if !tg_res.ok {
            // Telegram returns an error if content hasn't changed; ignore it
            if let Some(desc) = &tg_res.description {
                if desc.contains("message is not modified") {
                    return Ok(());
                }
            }
            bail!("Telegram editMessageText error: {:?}", tg_res.description);
        }
        Ok(())
    }

    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/answerCallbackQuery", self.base_url);

        #[derive(Serialize)]
        struct Answer<'a> {
            callback_query_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            text: Option<&'a str>,
        }

        let payload = Answer {
            callback_query_id,
            text,
        };

        let resp = self.client.post(&url).json(&payload).send().await?;
        let tg_res: TgResponse<bool> = resp.json().await?;

        if !tg_res.ok {
            bail!(
                "Telegram answerCallbackQuery error: {:?}",
                tg_res.description
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_update() {
        let json_data = r#"{
            "update_id": 10000,
            "message": {
                "message_id": 136,
                "from": {
                    "id": 1111111,
                    "is_bot": false,
                    "first_name": "TestUser",
                    "username": "tester"
                },
                "chat": {
                    "id": 1111111,
                    "type": "private"
                },
                "text": "/start"
            }
        }"#;

        let upd: Update = serde_json::from_str(json_data).unwrap();
        assert_eq!(upd.update_id, 10000);
        let msg = upd.message.unwrap();
        assert_eq!(msg.message_id, 136);
        assert_eq!(msg.from.unwrap().id, 1111111);
        assert_eq!(msg.text.unwrap(), "/start");
    }

    #[test]
    fn test_deserialize_callback_query() {
        let json_data = r#"{
            "update_id": 10001,
            "callback_query": {
                "id": "4382bf2893392889",
                "from": {
                    "id": 1111111,
                    "first_name": "TestUser"
                },
                "data": "cmd_start"
            }
        }"#;

        let upd: Update = serde_json::from_str(json_data).unwrap();
        let cb = upd.callback_query.unwrap();
        assert_eq!(cb.id, "4382bf2893392889");
        assert_eq!(cb.data.unwrap(), "cmd_start");
    }
}
