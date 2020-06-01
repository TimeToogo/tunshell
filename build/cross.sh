    1  apt update -y
    2  apt install curl -y
    3  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    4  apt install -y gcc-arm-linux-gnueabihf
    5  cd /tmp
    6  wget https://www.openssl.org/source/openssl-1.0.1t.tar.gz
    7  tar xzf openssl-1.0.1t.tar.gz
    8  export MACHINE=armv7
    9  export ARCH=arm
   10  apt install -y wget
   11  cd /tmp
   12  wget https://www.openssl.org/source/openssl-1.0.1t.tar.gz
   13  tar xzf openssl-1.0.1t.tar.gz
   14  export MACHINE=armv7
   15  export ARCH=arm
   16  export CC=arm-linux-gnueabihf-gcc
   17  cd openssl-1.0.1t && ./config shared && make && cd -
   18  apt install make
   19  cd openssl-1.0.1t && ./config shared && make && cd -
   20  ./config shared && make && cd -
   21  apt install -y binutils
   22  ar --help
   23  ./config shared && make && cd -
   24  cargo install cargo-add
   25  rustc
   26  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   27  source $HOME/.cargo/env
   28  cargo install cargo-add
   29  cc
   30  sudo apt install build-essential
   31  apt install build-essential
   32  cargo install cargo-add
   33  export OPENSSL_LIB_DIR=/tmp/openssl-1.0.1t/
   34  export OPENSSL_INCLUDE_DIR=/tmp/openssl-1.0.1t/include
   35  cargo new xx --bin
   36  cd xx
   37  mkdir .cargo
   38  cat > .cargo/config << EOF
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF

   39  cat > src/main.rs << EOF
extern crate openssl;

fn main() {
    println!("{}", openssl::version::version())
}
EOF

   40  ls -al
   41  mkdir src
   42  cat > src/main.rs << EOF
extern crate openssl;

fn main() {
    println!("{}", openssl::version::version())
}
EOF

   43  cargo add openssl
   44  ls
   45  ls -al
   46  cd ..
   47  rm -rf xx
   48  export USER=root
   49  export OPENSSL_LIB_DIR=/tmp/openssl-1.0.1t/
   50  export OPENSSL_INCLUDE_DIR=/tmp/openssl-1.0.1t/include
   51  cargo new xx --bin
   52  cd xx
   53  mkdir .cargo
   54  cat > .cargo/config << EOF
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF

   55  cat > src/main.rs << EOF
extern crate openssl;

fn main() {
    println!("{}", openssl::version::version())
}
EOF

   56  cargo add openssl
   57  cat Cargo.toml 
   58  apt install -y vim
   59  vim Cargo.toml 
   60  cargo build --target armv7-unknown-linux-gnueabihf
   61  rustup target add armv7-unknown-linux-gnueabihf
   62  cargo build --target armv7-unknown-linux-gnueabihf
   63  export OPENSSL_STATIC=1
   64  cargo build --target armv7-unknown-linux-gnueabihf
   65  ls -al target/armv7-unknown-linux-gnueabihf/debug/
   66  ls -alh target/armv7-unknown-linux-gnueabihf/debug/
   67  unset OPENSSL_STATIC
   68  ls -al target/armv7-unknown-linux-gnueabihf/debug/
   69  cargo build --target armv7-unknown-linux-gnueabihf
   70  ls -alh target/armv7-unknown-linux-gnueabihf/debug/
   71  echo $OPENSSL_STATIC
   72  export OPENSSL_STATIC=1
   73  ls -alh target/armv7-unknown-linux-gnueabihf/debug/
   74  cargo build --target armv7-unknown-linux-gnueabihf
   75  apt install -y crossbuild-essential-armhf
   76  cargo build --target armv7-unknown-linux-gnueabihf
   77  qemu-arm-static target/armv7-unknown-linux-gnueabihf/debug/xx
   78  apt install -y qemu-static
   79  apt install -y qemu-user-static
   80  qemu-arm-static target/armv7-unknown-linux-gnueabihf/debug/xx
   81  qemu-arm-static target/armv7-unknown-linux-gnueabihf/debug
   82  qemu-arm-static target/armv7-unknown-linux-gnueabihf/debug/xx
   83  ls -al /usr/arm-linux-gnueabi/
   84  qemu-arm-static -L /usr/arm-linux-gnueabi/ target/armv7-unknown-linux-gnueabihf/debug/xx
   85  qemu-arm -L /usr/arm-linux-gnueabi/ target/armv7-unknown-linux-gnueabihf/debug/xx
   86  unset LD_LIBRARY_PATH
   87  qemu-arm -L /usr/arm-linux-gnueabi/ target/armv7-unknown-linux-gnueabihf/debug/xx
   88  qemu-arm-static -L /usr/arm-linux-gnueabi/ target/armv7-unknown-linux-gnueabihf/debug/xx
   89  readelf
   90  readelf -a target/armv7-unknown-linux-gnueabihf/debug/xx
   91  readelf -a target/armv7-unknown-linux-gnueabihf/debug/xx | grep .so
   92  readelf -a target/armv7-unknown-linux-gnueabihf/debug/xx | grep ssl
   93  cd ..
   94  wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.17.tar.gz
   95  rm libsodium-1.0.17.tar.gz 
   96  wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.18-stable.tar.gz
   97  tar xzf libsodium-1.0.18-stable.tar.gz 
   98  cd libsodium-stable/
   99  ls
  100  find / gcc-arm-linux-gnueabihf
  101  find / -name=gcc-arm-linux-gnueabihf
  102  find / -name="gcc-arm-linux-gnueabihf"
  103  find / -name="*gcc-arm-linux-gnueabihf"
  104  man find
  105  find -help
  106  find / -wholename="*gcc-arm-linux-gnueabihf"
  107  find / -name "*gcc-arm-linux-gnueabihf"
  108  find / -name "*arm-linux"
  109  find / -name "*arm-linux*"
  110  export CC=arm-linux-gnueabihf-gcc
  111  env
  112  export LDFLAGS="--specs.nosys.specs"
  113  export CFLAGS="-Os"
  114  ./configure --host=arm-linux-gnueabihf --prefix=$PWD/libsodium
  115  cat config.log 
  116* ./con
  117  ./configure --host=arm-linux-gnueabihf --prefix=$PWD/libsodium
  118  unset CC
  119  ./configure --host=arm-linux-gnueabihf --prefix=$PWD/libsodium
  120  cat config.log
  121  export LDFLAGS='--specs=nosys.specs'
  122  ./configure --host=arm-linux-gnueabihf --prefix=$PWD/libsodium
  123  export CC=arm-linux-gnueabihf-gcc
  124  ./configure --build=x86_64-unknown-linux-gnu --host=arm-unknown-linux-gnueabihf --target=arm-unknown-linux-gnueabihf
  125  cat ./configure
  126  ./configure --build=x86_64-unknown-linux-gnu --host=arm-unknown-linux-gnueabihf --target=arm-unknown-linux-gnueabihf
  127  ./configure --host=arm-linux-gnueabihf 
  128  arm-linux-gnueabihf-gcc
  129  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF | tee hello.c
echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF; | tee hello.c



  130  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF | tee hello.c
echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF;





  131  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF | tee hello.c
echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
 EOF;

  132  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF

  133  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF

  134  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
 EOF



  135  echo << EOF
// Minimal C example
#include <stdio.h>
int main()
{
   printf("This works\n");
   return 0;
}
EOF

  136  ;
  137  echo <<<EOF
  138  // Minimal C example
  139  #include <stdio.h>
  140  int main()
  141  {    printf("This works\n");
  142  }
  143  EOF
  144  vim hello.c
  145  arm-linux-gnueabihf-gcc hello.c 
  146  hello
  147  ./hello
  148  ls -al
  149  ./a.out
  150  echo $CC
  151  export CC="gcc"
  152  ./configure --host=arm-linux-gnueabihf 
  153  cat config.log
  154  ./configure --host=arm-linux-gnueabihf 
  155  cat config.log
  156  unset LDFLAGS
  157* ./configure --host=arm-linux-gnueabihf --p
  158  make install
  159  ls -al /tmp/libsodium-
  160  ./configure --host=arm-linux-gnueabihf --prefix=$PWD/libsodium
  161  ./configure --host=arm-linux-gnueabihf --prefix=/tmp/libsodium
  162  make install
  163  cd ..
  164  ls -al libsodium
  165  ls -al libsodium/lib/
  166  export SODIUM_LIB_DIR="/tmp/libsodium/lib"
  167  export SODIUM_STATIC=1
  168  cd /app/
  169  cargo build --target armv7-unknown-linux-gnueabihf
  170  export CC=arm-linux-gnueabihf-gcc 
  171  cargo build --target armv7-unknown-linux-gnueabihf
  172  vim ~/.cargo/registry/src/github.com-1ecc6299db9ec823/thrussh-0.26.1/src/client/mod.rs 
  173  cargo build --target armv7-unknown-linux-gnueabihf
  174  rm -rf ./target/
  175  cargo build --target armv7-unknown-linux-gnueabihf
  176  ls -al /usr/bin | grep arm
  177  export LD="arm-linux-gnueabihf-gcc"
  178  cargo build --target armv7-unknown-linux-gnueabihf
  181  file /app/target/armv7-unknown-linux-gnueabihf/debug/deps/libthrussh_libsodium-67a2964f352b86ca.rlib
  182  file /app/target/armv7-unknown-linux-gnueabihf/debug/deps/libthrussh_libsodium-67a2964f352b86ca.rmeta
  183  cat /app/target/armv7-unknown-linux-gnueabihf/debug/deps/libthrussh_libsodium-67a2964f352b86ca.rmeta
  184  env 
  185  cargo build thrush-libsodium
  186  cargo build --help
  187  cargo build -p thrush-libsodium
  188  cargo build -p thrussh-libsodium
  189  cargo build -p thrussh-libsodium  --target armv7-unknown-linux-gnueabihf
  190  unset LD
  191  unset CC
  192  cargo build -p thrussh-libsodium  --target armv7-unknown-linux-gnueabihf
  193  mkdir .cargo
  194  cat > .cargo/config << EOF
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF

  195  cargo build -p thrussh-libsodium  --target armv7-unknown-linux-gnueabihf
  196  cargo build  --target armv7-unknown-linux-gnueabihf
  197  cd /tmp/libsodium/
  198  ls
  199  cd ../libsodium-stable/
  200  ls -al
  201  export LDFLAGS='--specs=nosys.specs'
  202  ./configure --host=arm-linux-gnueabihf --prefix=/tmp/libsodium
  203  cat config.log
  204  cd ../libsodium/lib
  205  file libsodium.a 
  206  file libsodium.so
  207  file libsodium.so.23.3.0 
  208  cd ../../libsodium-stable
  209  export CC=arm-linux-gnueabihf-gcc 
  210  unset LDFLAGS
  211  ./configure --host=arm-linux-gnueabihf --prefix=/tmp/libsodium
  212  make install
  213  export LD=arm-linux-gnueabihf-gcc 
  214  make install
  215  make clean
  216  make install
  217  file crypto_aead/xchacha20poly1305/sodium/libsodium_la-aead_xchacha20poly1305.lo
  218  file libsodium/crypto_aead/xchacha20poly1305/sodium/libsodium_la-aead_xchacha20poly1305.lo
  219  ls /tmp/libsodium-stable/src/libsodium
  220  make clean install
  221  cd /tmp/libsodium/lib
  222  file libsodium.so.23.3.0 
  223  cd /app/
  224  ls -al
  225  cargo build  --target armv7-unknown-linux-gnueabihf
  226  rm -rf target/
  227  cargo build  --target armv7-unknown-linux-gnueabihf
  228  file target/armv7-unknown-linux-gnueabihf/bin/client
  229  file target/armv7-unknown-linux-gnueabihf/debug/main
  230  ./target/armv7-unknown-linux-gnueabihf/debug/main
  231  qemu-arm-static -L /usr/arm-linux-gnueabi/ target/armv7-unknown-linux-gnueabihf/debug/main
  232  readelf target/armv7-unknown-linux-gnueabihf/debug/main
  233  -a readelf target/armv7-unknown-linux-gnueabihf/debug/main
  234  -a readelf -a target/armv7-unknown-linux-gnueabihf/debug/main
  235  readelf -a target/armv7-unknown-linux-gnueabihf/debug/main
  236  readelf -a target/armv7-unknown-linux-gnueabihf/debug/main | openssl
  237  readelf -a target/armv7-unknown-linux-gnueabihf/debug/main | grep openssl
  238  cat ~/. bash_history
  239  cat ~/.bash_history
  240  echo $HISTFILE
  241  cat /root/.bash_history
  242  history
  243  history > cross.sh
