
#/bin/bash

SESSION_ID="TEST"

ATTEMPT_WAIT=5 # seconds
MAX_ATTEMPTS=2

set -e
set -x

echo "Configuring local to connect to pipeline"

sudo apt-get update

if [[ ! -x "$(which ssh)" ]]; then
    echo "Installing OpenSSH"
    sudo apt-get install -y openssh-client
else
    echo "SSH Exists"
fi;

mkdir -p ~/.ssh/
ssh-keygen -t rsa -b 4096 -P '' -f ~/.ssh/relay

# POST public key to https://relay.debugmypipeline.com/{uuid}
curl -XPUT "https://debug-pipeline-test.s3-ap-southeast-2.amazonaws.com/${SESSION_ID}/public_key" --data-binary @"$HOME/.ssh/relay.pub"

for i in {1..$ATTEMPT_WAIT}
do
    echo "Waiting for username..."
    curl --fail "https://debug-pipeline-test.s3-ap-southeast-2.amazonaws.com/${SESSION_ID}/user" -o /tmp/user_name
    RESULT=$?
    if [[ "$RESULT" == "0" ]];
    then
        PIPELINE_USER="$(cat /tmp/user_name)"
        echo "Username retrieved: $PIPELINE_USER"
        break
    fi
done

echo "Connecting..."
ssh -N -L 1122:127.0.0.1:12345 relay@relay.debugmypipeline.com &
TUNNEL_PID=$!

ssh -o StrictHostKeyChecking=no -i ~/.ssh/relay $PIPELINE_USER@localhost -p 1122

echo "Terminating"
kill -SIGTERM $TUNNEL_PID

