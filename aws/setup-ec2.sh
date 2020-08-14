
#!/bin/bash

set -e

echo "Setting up tunshell server..."

if [[ ! -x "$(command -v docker)" ]];
then
    sudo apt-get update
    sudo apt-get install -y docker.io docker-compose
    sudo usermod -aG docker ubuntu
fi

if [[ ! -f mongo_password ]];
then
    cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w 32 | head -n 1 > mongo_password
fi

export MONGO_PASSWORD=$(cat mongo_password)
export RELAY_DOMAIN=$(cat relay_domain)

curl https://raw.githubusercontent.com/TimeToogo/tunshell/master/aws/docker-compose.yml > docker-compose.yml
curl https://raw.githubusercontent.com/TimeToogo/tunshell/master/aws/mongo_init.js > mongo_init.js
curl https://raw.githubusercontent.com/TimeToogo/tunshell/master/aws/nginx.conf > nginx.conf

mkdir -p config/nginx/site-confs/
mv nginx.conf config/nginx/site-confs/default
sed -i "s/{{password}}/$MONGO_PASSWORD/g" mongo_init.js

sudo service docker start
sg docker -c "docker-compose pull"
# start services gradually as not to run out of memory on small instances
# and allow extra time for lets encrypt proxy to generate cert 
sg docker -c "docker-compose up -d reverse_proxy"
sleep 60
sg docker -c "docker-compose up -d mongo"
sleep 30
sg docker -c "docker-compose up -d relay"
sleep 30
sg docker -c "docker-compose up -d watchtower"

echo "done!"
