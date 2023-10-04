# Yande Popular

## 介绍
自动下载[Yande.re](https://yande.re)的每日热门图片,并通过机器人接口上传至[VoceChat](https://voce.chat/)频道或者[Matrix](https://matrix.org/)房间。

## 使用
#### Docker
For VoceChat
```
docker run -d --name yande_popular -e API_KEY="xxxxxxxxxxxxxxxxx" -e SERVER_DOMAIN="http://voce.chat" -e CHANNEL_ID="1" -v ./yande_popular:/yande_popular --restart unless-stopped chikage/yande_popular:voce
```
For Matrix
```
docker run -d --name yande_popular -e HOME_SERVER_URL = "https://xxx.xxx" -e ROOM_ID = "!PWPurdafsdfasd:xx.xxx" -e USER = "x" -e PASSWORD = "x" -v ./yande_popular:/yande_popular --restart unless-stopped chikage/yande_popular:matrix
```