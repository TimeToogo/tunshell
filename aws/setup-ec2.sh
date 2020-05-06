
#/bin/bash

set -e

echo "Setting up debug-my-pipeline server..."

if [[ ! $(which docker) ]]
then
    sudo apt-get update
    sudo apt-get install -y docker.io docker-compose
fi

if [[ ! -f mongo_password ]];
then
    cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w 32 | head -n 1 > mongo_password
fi

export MONGO_PASSWORD=$(cat mongo_password)

wget https://raw.githubusercontent.com/TimeToogo/debug-my-pipeline/master/aws/docker-compose.yml

mkdir mongo/
wget https://raw.githubusercontent.com/TimeToogo/debug-my-pipeline/master/aws/mongo/001_init.js mongo/

sed -i "s/{{password}}/$MONGO_PASSWORD/g" mongo/001_init.js

docker-compose up -d