# Yande Popular

## 介绍
自动下载[Yande.re](https://yande.re)的每日热门图片,并通过机器人接口上传至[VoceChat](https://voce.chat/)频道

## 使用
#### Docker
```
docker run -d --name yande_popular -e API_KEY="xxxxxxxxxxxxxxxxx" -e SERVER_DOMAIN="http://voce.chat" -e CHANNEL_ID="1" -v ~/yande_popular:/yande_popular --restart unless-stopped chikage/yande_popular:latest
```