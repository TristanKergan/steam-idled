use crate::config::BotConfig;
use crate::metrics::{format_duration, SystemMetrics};
use crate::tg::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, TgClient};
use anyhow::Result;
use protocol::{Config, IpcClient, Request, Response};
use std::path::PathBuf;
use tracing::{info, warn};

pub struct BotHandler {
    cfg: BotConfig,
    tg: TgClient,
    socket_path: PathBuf,
}

impl BotHandler {
    pub fn new(cfg: BotConfig, tg: TgClient) -> Self {
        let socket_path = cfg
            .socket_path
            .clone()
            .unwrap_or_else(Config::default_socket_path);
        Self {
            cfg,
            tg,
            socket_path,
        }
    }

    pub fn is_authorized(&self, user_id: i64) -> bool {
        user_id == self.cfg.admin_user_id
    }

    pub async fn handle_message(&self, msg: Message) -> Result<()> {
        let user = match &msg.from {
            Some(u) => u,
            None => return Ok(()),
        };

        if !self.is_authorized(user.id) {
            warn!(
                "Unauthorized access attempt from user id={} (@{:?})",
                user.id, user.username
            );
            let _ = self
                .tg
                .send_message(
                    msg.chat.id,
                    "⛔ <b>Доступ запрещён.</b> Бот настроен на работу только с владельцем.",
                    None,
                )
                .await;
            return Ok(());
        }

        let text = msg.text.as_deref().unwrap_or("").trim();
        let cmd = text.split_whitespace().next().unwrap_or("");

        match cmd {
            "/start" | "/status" | "/farm" => {
                let (status_text, keyboard) = self.build_status_view().await;
                self.tg
                    .send_message(msg.chat.id, &status_text, Some(&keyboard))
                    .await?;
            }
            "/stop" => {
                let res = self.send_ipc_request(Request::Stop { appids: None }).await;
                let reply = match res {
                    Ok(_) => "⏹ <b>Фарм остановлен.</b> Включён ручной режим.\nТеперь можно безопасно играть на ПК.",
                    Err(e) => &format!("❌ Ошибка остановки: {}", e),
                };
                let (_, keyboard) = self.build_status_view().await;
                self.tg
                    .send_message(msg.chat.id, reply, Some(&keyboard))
                    .await?;
            }
            "/logs" => {
                let logs_text = self.get_recent_logs(15).await;
                self.tg.send_message(msg.chat.id, &logs_text, None).await?;
            }
            "/help" => {
                let help_text = "\
📖 <b>Команды бота:</b>
/status — показать статус фарма и сервера
/stop — остановить фарм (для игры на ПК)
/logs — последние строки логов
/help — это сообщение

Вы также можете использовать кнопки под сообщением статуса.";
                self.tg.send_message(msg.chat.id, help_text, None).await?;
            }
            _ => {
                let (status_text, keyboard) = self.build_status_view().await;
                self.tg
                    .send_message(msg.chat.id, &status_text, Some(&keyboard))
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn handle_callback_query(&self, cb: CallbackQuery) -> Result<()> {
        if !self.is_authorized(cb.from.id) {
            warn!(
                "Unauthorized callback query from user id={} (@{:?})",
                cb.from.id, cb.from.username
            );
            let _ = self
                .tg
                .answer_callback_query(&cb.id, Some("⛔ Доступ запрещён"))
                .await;
            return Ok(());
        }

        let action = cb.data.as_deref().unwrap_or("");
        info!("Handling callback action: {}", action);

        match action {
            "cmd_start" => {
                let res = self
                    .send_ipc_request(Request::Start { appids: vec![] })
                    .await;
                match res {
                    Ok(_) => {
                        self.tg
                            .answer_callback_query(&cb.id, Some("▶️ Фарм запущен"))
                            .await?;
                    }
                    Err(e) => {
                        self.tg
                            .answer_callback_query(&cb.id, Some(&format!("❌ Ошибка: {}", e)))
                            .await?;
                    }
                }
            }
            "cmd_stop" => {
                let res = self.send_ipc_request(Request::Stop { appids: None }).await;
                match res {
                    Ok(_) => {
                        self.tg
                            .answer_callback_query(&cb.id, Some("⏹ Фарм остановлен"))
                            .await?;
                    }
                    Err(e) => {
                        self.tg
                            .answer_callback_query(&cb.id, Some(&format!("❌ Ошибка: {}", e)))
                            .await?;
                    }
                }
            }
            "cmd_refresh" => {
                self.tg
                    .answer_callback_query(&cb.id, Some("🔄 Статус обновлен"))
                    .await?;
            }
            "cmd_logs" => {
                self.tg
                    .answer_callback_query(&cb.id, Some("📜 Загрузка логов..."))
                    .await?;
                if let Some(msg) = &cb.message {
                    let logs_text = self.get_recent_logs(15).await;
                    self.tg.send_message(msg.chat.id, &logs_text, None).await?;
                }
                return Ok(());
            }
            _ => {
                self.tg.answer_callback_query(&cb.id, None).await?;
            }
        }

        // Update the status message in-place
        if let Some(msg) = &cb.message {
            let (status_text, keyboard) = self.build_status_view().await;
            let _ = self
                .tg
                .edit_message_text(msg.chat.id, msg.message_id, &status_text, Some(&keyboard))
                .await;
        }

        Ok(())
    }

    async fn send_ipc_request(&self, req: Request) -> Result<Response> {
        let mut client = IpcClient::connect(&self.socket_path).await?;
        let resp = client.call(&req).await?;
        Ok(resp)
    }

    async fn get_recent_logs(&self, lines: usize) -> String {
        match self.send_ipc_request(Request::Logs { lines }).await {
            Ok(Response::Logs(payload)) => {
                if payload.lines.is_empty() {
                    "📜 <i>Логи пусты</i>".to_string()
                } else {
                    let joined = payload.lines.join("\n");
                    format!(
                        "📜 <b>Последние логи steam-idled:</b>\n<pre>{}</pre>",
                        escape_html(&joined)
                    )
                }
            }
            Ok(other) => format!("⚠️ Неожиданный ответ логов: {:?}", other),
            Err(e) => format!("❌ Не удалось получить логи: {}", e),
        }
    }

    async fn build_status_view(&self) -> (String, InlineKeyboardMarkup) {
        let metrics = SystemMetrics::collect();
        let daemon_status = match self.send_ipc_request(Request::Status).await {
            Ok(Response::Status(s)) => Some(s),
            _ => None,
        };

        let mut out = String::new();

        // 1. Steam & Idle section
        let idle_active = daemon_status
            .as_ref()
            .map(|s| s.idle_active)
            .unwrap_or(false);

        if let Some(ref st) = daemon_status {
            let state_icon = if st.idle_active {
                "🟢"
            } else if st.manual_stop {
                "⏸"
            } else if !st.steam_connected {
                "🟡"
            } else {
                "⚪"
            };

            let state_desc = if st.idle_active {
                "АКТИВЕН (в игре)"
            } else if st.manual_stop {
                "ОСТАНОВЛЕН (ручной режим)"
            } else if !st.steam_connected {
                "ОЖИДАНИЕ STEAM"
            } else if st.real_cs2_running {
                "ПАУЗА (запущен настоящий CS2)"
            } else {
                "В ПРОСТОЕ"
            };

            out.push_str(&format!(
                "{} <b>Фарм:</b> <code>{}</code>\n",
                state_icon, state_desc
            ));

            if !st.active_games.is_empty() {
                for g in &st.active_games {
                    out.push_str(&format!(
                        "🕹 <b>Игра:</b> {} (<code>{}</code>)\n",
                        escape_html(&g.name),
                        g.appid
                    ));
                    out.push_str(&format!(
                        "⏱ <b>Время сессии:</b> <code>{}</code>\n",
                        format_duration(g.session_duration_secs)
                    ));
                }
            } else {
                out.push_str("🕹 <b>Активных игр:</b> нет\n");
            }

            let steam_icon = if st.steam_connected { "✅" } else { "❌" };
            out.push_str(&format!(
                "🎮 <b>Steam клиент:</b> {} {}\n",
                steam_icon,
                if st.steam_connected {
                    "Подключен"
                } else {
                    "Не найден"
                }
            ));
        } else {
            out.push_str("❌ <b>steam-idled:</b> <code>НЕ ЗАПУЩЕН</code>\n");
            out.push_str("<i>Запустите службу: systemctl --user start steam-idled</i>\n");
        }

        out.push('\n');

        // 2. Hardware metrics section
        out.push_str("💻 <b>Сервер:</b>\n");

        // CPU temp
        if let Some(temp) = metrics.cpu_temp_c {
            let temp_icon = if temp < 55.0 {
                "❄️"
            } else if temp < 75.0 {
                "🌡"
            } else {
                "🔥"
            };
            out.push_str(&format!("├ {} <b>CPU:</b> {:.1}°C\n", temp_icon, temp));
        } else {
            out.push_str("├ 🌡 <b>CPU:</b> N/A\n");
        }

        // RAM
        let used_gb = metrics.ram_used_mb as f64 / 1024.0;
        let total_gb = metrics.ram_total_mb as f64 / 1024.0;
        out.push_str(&format!(
            "├ 🧠 <b>RAM:</b> {:.1} / {:.1} GB ({}%)\n",
            used_gb, total_gb, metrics.ram_percent
        ));

        // Battery / Power
        if let Some(bat) = metrics.battery {
            let bat_icon = if bat.ac_online { "⚡" } else { "🔋" };
            out.push_str(&format!(
                "├ {} <b>Батарея:</b> {}% [{} {}]\n",
                bat_icon,
                bat.capacity_percent,
                if bat.ac_online {
                    "Сеть 🔌"
                } else {
                    "АКБ"
                },
                bat.status
            ));
        } else {
            out.push_str("├ 🔌 <b>Питание:</b> Сеть 220V\n");
        }

        // Uptime
        out.push_str(&format!(
            "└ ⏱ <b>Аптайм ОС:</b> {}\n",
            format_duration(metrics.uptime_secs)
        ));

        // 3. Inline keyboard
        let toggle_btn = if idle_active {
            InlineKeyboardButton::callback("⏹ Остановить фарм", "cmd_stop")
        } else {
            InlineKeyboardButton::callback("▶️ Запустить фарм", "cmd_start")
        };

        let refresh_btn = InlineKeyboardButton::callback("🔄 Обновить", "cmd_refresh");
        let logs_btn = InlineKeyboardButton::callback("📜 Логи", "cmd_logs");

        let keyboard =
            InlineKeyboardMarkup::new(vec![vec![toggle_btn, refresh_btn], vec![logs_btn]]);

        (out, keyboard)
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(
            escape_html("Hello <world> & friends"),
            "Hello &lt;world&gt; &amp; friends"
        );
    }

    #[test]
    fn test_is_authorized() {
        let cfg = BotConfig {
            bot_token: "token".into(),
            admin_user_id: 12345,
            socket_path: None,
            poll_timeout_secs: 30,
        };
        let tg = TgClient::new("fake_token");
        let handler = BotHandler::new(cfg, tg);

        assert!(handler.is_authorized(12345));
        assert!(!handler.is_authorized(99999));
    }
}
