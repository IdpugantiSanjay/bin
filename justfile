
publish_docker_image:
    docker build . -t isanjay112/bin
    docker image push isanjay112/bin