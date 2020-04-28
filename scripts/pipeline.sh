
#/bin/bash

SESSION_ID="TEST"

ATTEMPT_WAIT=10 # seconds
MAX_ATTEMPTS=60

set -x

echo "Configuring pipeline for SSH access"

sudo apt-get update

if [[ ! -x "$(which sudo)" ]]; then
    echo "Installing sudo"
    sudo apt-get install -y sudo
else
    echo "sudo already installed"
fi;


if [[ ! -x "$(which sshd)" ]]; then
    echo "Installing OpenSSH"
    sudo apt-get install -y openssh-server
else
    echo "OpenSSH already installed"
fi;

if [[ ! -x "$(which curl)" ]]; then
    echo "Installing curl"
    sudo apt-get install -y curl
else
    echo "curl already installed"
fi;

curl -XGET "https://ipinfo.io/json"

curl -XPUT "https://debug-pipeline-test.s3-ap-southeast-2.amazonaws.com/${SESSION_ID}/user" --data-binary "$(whoami)"
mkdir -p ~/.ssh/
sudo chmod go-w ~/
sudo chmod 700 ~/.ssh

for i in {1..$ATTEMPT_WAIT}
do
    echo "Waiting for public key..."
    curl --fail "https://debug-pipeline-test.s3-ap-southeast-2.amazonaws.com/${SESSION_ID}/public_key" -o /tmp/user_pub_key
    RESULT=$?
    if [[ "$RESULT" == "0" ]];
    then
        echo "Public key received!"
        cat /tmp/user_pub_key >> ~/.ssh/authorized_keys
        sudo chmod 600 ~/.ssh/authorized_keys
        cat ~/.ssh/authorized_keys
        break
    fi
done;

echo "Waiting for connection..."
ssh -o StrictHostKeyChecking=no -N -R 12345:127.0.0.1:22 relay@relay.debugmypipeline.com &
PID=$!
sudo tail -f /var/log/auth.log
sleep 600
kill -SIGTERM $PID
echo "Terminating"


# TODO timeout
