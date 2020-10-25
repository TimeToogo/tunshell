Relay Server
============

The relay server serves the clients to establish network connectivity between themselves.
Providing them with the address and port information to attempt direct p2p connections and also support the proxying of traffic between clients if direct connections are not possible.
This allows the clients to operate even when behind restrictive firewalls or NAT devices.

It is composed of:

 - A custom server application, the [tunshell-server](../tunshell-server)
 - A SQLite database which persists the session keys
 - A [Nginx + Let's Encrypt](https://github.com/linuxserver/docker-letsencrypt) which generates SSL certificates and provides http -> https redirection

These services are containerised and can be easily spun up by docker compose.

## Self-hosting your own relay server

### On AWS

The easiest way to spin up your own relay server is to use the CloudFormation template which is scripted to configure a new relay server in your own AWS environment. 

1. Go to Cloud Formation console: https://console.aws.amazon.com/cloudformation/home
2. Create a new stack and select 'Upload a template file'
3. Upload [this template](./stack.yml)
4. Fill out the parameters and let the stack initialise
   - This stack will configure an EC2 instance
   - Create DNS records for the server (for LetsEncrypt validation)
   - Bootstrap the instance with docker and the containers

### Elsewhere

If you are hosting your server outside of AWS you will have to provision and configure the relay server yourself.

You may use the [bootstrap script](./setup-ec2.sh) and [docker-compose.yml](./docker-compose.yml) files as a starting point.

## Using your self-hosted relay server

Unfortunately the current tunshell.com website is hard-coded to use the standard relay.tunshell.com server to generate session keys so these will have to be created manually. 
An easy way to create a new pair of session keys is via `curl`:

```sh
curl -XPOST https://relay.yourdomain.com/api/sessions
```

Which will generate a pair of session keys:

```json
{ "peer1_key": "M485rfdQg8h24byfwkEsc", "peer2_key": "FDxbfDDd67fRFd5skflmc" }
```

Additionally you will have to pass an additional argument to the client init scripts to override the default relay server:

```sh
# Local
sh <(curl -sSf https://lets.tunshell.com/init.sh) L M485rfdQg8h24byfwkEsc YcO8WqyFIEVv8mxReYugGq relay.yourdomain.com

# Target
curl -sSf https://lets.tunshell.com/init.sh | sh -s -- T FDxbfDDd67fRFd5skflmc YcO8WqyFIEVv8mxReYugGq relay.yourdomain.com
```
