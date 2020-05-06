
#/bin/bash

set -e

echo "Setting up debug-my-pipeline server..."

if [[ ! $(which docker) ]]
then
    sudo apt-get update
    sudo apt-get install -y docker.io docker-compose
    sudo usermod -aG docker $USER
fi


if [[ ! -f mongo_password ]];
then
    cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w 32 | head -n 1 > mongo_password
fi

export MONGO_PASSWORD=$(cat mongo_password)

curl https://raw.githubusercontent.com/TimeToogo/debug-my-pipeline/master/aws/docker-compose.yml > docker-compose.yml
curl https://raw.githubusercontent.com/TimeToogo/debug-my-pipeline/master/aws/mongo_init.js > mongo_init.js
curl https://raw.githubusercontent.com/TimeToogo/debug-my-pipeline/master/aws/nginx.conf > nginx.conf

mkdir -p config/nginx/site-confs/
mv nginx.conf config/nginx/site-confs/default

sed -i "s/{{password}}/$MONGO_PASSWORD/g" mongo_init.js

sudo service docker start
sg docker -c "docker-compose up -d"