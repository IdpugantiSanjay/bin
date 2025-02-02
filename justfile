publish_docker_image:
    docker build . -t isanjay112/bin
    docker image push isanjay112/bin

dev:
    xdg-open http://127.0.0.1:8820
    cargo watch -x run

release:
    cargo build --release


restart: release
    sudo systemctl restart bin-api.service

build:
    cargo build