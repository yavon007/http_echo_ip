# http_echo_ip
一个只会输出IP的http服务，配合DDNS使用

参数
```
-l, --listen <LISTEN>  监听地址 [default: 127.0.0.1]
-p, --port <PORT>      监听端口号 [default: 80]
```
cli: `http_echo_ip -l 127.0.0.1 -p 23343`

docker: `docker run --name echo_ip -d --restart always --network host evlan/http_echo_ip -l 127.0.0.1 -p 23343`

配合nginx反代
```
    location = /ip {
        proxy_pass http://127.0.0.1:23343;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header REMOTE-HOST $remote_addr;
    }
```