Tunshell (Beta)
===============

https://tunshell.com

Tunshell is a free service which allows you to securely shell into remote hosts with near-zero setup.
The project is predominately written in [Rust](https://www.rust-lang.org/) and this repository contains
the components which make up the system.

## Why?

> Why would I use this when I am already able to use my well-established SSH client?

Good question, you wouldn't! The use case for tunshell is for quick and dirty, ad-hoc connections to hosts which you may not have SSH access to, or not even the ability to install an SSH daemon at all. The beauty of tunshell is that it's client is a statically-linked, pre-compiled binary which can be installed by simply downloading it via a one-liner script. This makes it ideal to quickly debug environments you normally wouldn't have shell access to, some examples:

#### Debugging Deployment Pipelines

Tunshell allows you to remote shell into GitHub Actions, BitBucket Pipelines etc by inserting a one-liner into your build scripts. If you've ever spent hours trying to track down an issue on a deployment pipeline that you couldn't replicate locally because of subtle environmental differences, this could come in handy.

#### Serverless Functions

Tunshell even supports extremely limited environments such as AWS Lambda or Google Cloud Functions. 

## How does it work?

Tunshell is comprised of 3 main components:

 - [Relay Server](./tunshell-server): a server which is able to coordinate with clients to establish connectivity
 - [Client Binary](./tunshell-client): a portable binary which is able to downloaded an run on two hosts acting as a shell server or client.
 - [Website](./website): The user interface for configuring a remote shell session with the relay server and providing install scripts for the client.

The process is kicked off using [tunshell.com](https://tunshell.com) by generating install scripts relevant to the local and target environments.

#### Install Script

![Install Script](https://app.lucidchart.com/publicSegments/view/8ad48e9c-299b-4d55-8c95-2d1aa07475c6/image.png)

