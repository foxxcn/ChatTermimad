use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::io::Write;
use termimad::MadSkin;

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct Choice {
    delta: Option<DeltaContent>,
}

#[derive(Debug, Deserialize)]
struct DeltaContent {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("DEEPSEEK_API_KEY").expect("请设置 DEEPSEEK_API_KEY 环境变量");
    let client = reqwest::Client::new();
    let mut messages = vec![Message {
        role: "system".to_string(),
        content: "You are a helpful assistant".to_string(),
    }];

    let mut temperature = 1.0; // 默认温度值
    let mut max_tokens = 4096; // 默认最大输出长度
    let stdin = std::io::stdin();
    let mut input = String::new();

    println!("欢迎使用 DeepSeek AI 聊天！");
    println!("特殊命令：");
    println!("  /temp <数值>     - 设置 temperature (0.0-1.5)");
    println!("  /mode <模式>     - 快速设置预定义温度:");
    println!("                     code(0.0), data(1.0), chat(1.3),");
    println!("                     translate(1.3), creative(1.5)");
    println!("  /tokens <数值>   - 设置最大输出长度 (1-8192)");
    println!("  /help           - 显示帮助信息");
    println!("----------------------------------------");

    loop {
        input.clear();
        print!("You: ");
        std::io::stdout().flush()?;
        stdin.read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // 处理特殊命令
        if input.starts_with('/') {
            match input.split_whitespace().collect::<Vec<_>>().as_slice() {
                ["/temp", value] => {
                    if let Ok(temp) = value.parse::<f32>() {
                        if (0.0..=1.5).contains(&temp) {
                            temperature = temp;
                            println!("Temperature 已设置为: {}", temperature);
                        } else {
                            println!("Temperature 必须在 0.0 到 1.5 之间");
                        }
                    } else {
                        println!("无效的 temperature 值");
                    }
                    continue;
                }
                ["/mode", mode] => {
                    temperature = match *mode {
                        "code" => 0.0,
                        "data" => 1.0,
                        "chat" => 1.3,
                        "translate" => 1.3,
                        "creative" => 1.5,
                        _ => {
                            println!("未知模式。可用模式: code, data, chat, translate, creative");
                            continue;
                        }
                    };
                    println!("模式已切换,temperature 设置为: {}", temperature);
                    continue;
                }
                ["/tokens", value] => {
                    if let Ok(tokens) = value.parse::<usize>() {
                        if (1..=8192).contains(&tokens) {
                            max_tokens = tokens;
                            println!("最大输出长度已设置为: {} tokens", max_tokens);
                        } else {
                            println!("最大输出长度必须在 1 到 8192 之间");
                        }
                    } else {
                        println!("无效的 tokens 值");
                    }
                    continue;
                }
                ["/help"] => {
                    println!("可用命令：");
                    println!("  /temp <数值>     - 设置 temperature (0.0-1.5)");
                    println!("  /mode <模式>     - 快速设置预定义温度:");
                    println!("                     code(0.0), data(1.0), chat(1.3),");
                    println!("                     translate(1.3), creative(1.5)");
                    println!("  /tokens <数值>   - 设置最大输出长度 (1-8192)");
                    println!("  /help           - 显示此帮助信息");
                    continue;
                }
                _ => {
                    println!("未知命令。输入 /help 查看可用命令。");
                    continue;
                }
            }
        }

        messages.push(Message {
            role: "user".to_string(),
            content: input.to_string(),
        });

        let mut response = client
            .post("https://api.deepseek.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({
                "model": "deepseek-chat",
                "messages": messages,
                "temperature": temperature,
                "max_tokens": max_tokens,
                "stream": true
            }))
            .send()
            .await?;

        print!("DeepSeek: ");
        std::io::stdout().flush()?;

        let mut accumulated_content = String::new();
        let mut buffer = String::new();
        let mut is_code_block = false;
        let mut current_line = String::new();

        // 自定义皮肤
        let mut skin = MadSkin::default();
        skin.set_headers_fg(termimad::crossterm::style::Color::Cyan);
        skin.bold.set_fg(termimad::crossterm::style::Color::Yellow);
        skin.italic
            .set_fg(termimad::crossterm::style::Color::Magenta);
        skin.code_block
            .set_fg(termimad::crossterm::style::Color::Green);

        while let Some(chunk) = response.chunk().await? {
            // 使用 String::from_utf8_lossy 来处理无效的 UTF-8 数据
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        continue;
                    }

                    if let Ok(response) = serde_json::from_str::<ApiResponse>(data) {
                        if let Some(content) = response
                            .choices
                            .first()
                            .and_then(|choice| choice.delta.as_ref())
                            .and_then(|delta| delta.content.as_ref())
                        {
                            // 处理代码块
                            if content.contains("```") {
                                is_code_block = !is_code_block;
                                if !buffer.is_empty() {
                                    print!("\r\x1B[K");
                                    skin.print_text(&buffer);
                                    buffer.clear();
                                }
                                continue;
                            }

                            // 累积内容
                            current_line.push_str(content);
                            accumulated_content.push_str(content);

                            // 检查是否需要渲染
                            if content.contains('\n') {
                                buffer.push_str(&current_line);
                                current_line.clear();

                                // 渲染完整段落或块
                                if !buffer.is_empty() {
                                    print!("\r\x1B[K");
                                    if is_code_block {
                                        // 对代码块使用特殊格式化
                                        buffer = format!("```\n{}\n```", buffer);
                                    }
                                    skin.print_text(&buffer);
                                    buffer.clear();
                                }
                            } else if !is_code_block
                                && (content.contains('。')
                                    || content.contains('!')
                                    || content.contains('?'))
                            {
                                // 在普通文本中遇到句末标点时渲染
                                buffer.push_str(&current_line);
                                current_line.clear();
                                print!("\r\x1B[K");
                                skin.print_text(&buffer);
                                buffer.clear();
                            }
                        }
                    }
                }
            }
        }

        // 渲染剩余内容
        if !current_line.is_empty() {
            buffer.push_str(&current_line);
        }
        if !buffer.is_empty() {
            print!("\r\x1B[K");
            if is_code_block {
                // 确保最后的代码块也正确格式化
                buffer = format!("```\n{}\n```", buffer);
            }
            skin.print_text(&buffer);
        }
        println!();

        messages.push(Message {
            role: "assistant".to_string(),
            content: accumulated_content,
        });
    }
}
