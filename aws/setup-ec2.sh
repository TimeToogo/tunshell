
#/bin/bash

set -e

echo "Setting up debug-my-pipeline server"

sudo apt-get update

sudo apt-get install openssh-server

sudo useradd relay
sudo passwd -d relay
mkdir -p /home/relay

echo -e "Match User relay \n\
    PermitTTY no \n\
    ForceCommand /sbin/nologin \n\
    ChrootDirectory %h \n\
    AllowTcpForwarding yes \n\
    GatewayPorts yes \n\
    PasswordAuthentication yes \n\
    PermitEmptyPasswords yes \n\
    PubkeyAuthentication no\n" | sudo tee -a /etc/ssh/sshd_config > /dev/null

sudo service sshd reload

# Dream

# Goto https://debugmypipeline.com and click start to generate URL's

# In pipeline 

# curl https://lets.debugmypipeline.com/fdsdsfds-fdsfdsf-fdsfds-fsdffsdf | sh


# On local

# curl https://lets.debugmypipeline.com/fdsdsfds-fdsfdsf-fdsfds-fsdffsdf | sh