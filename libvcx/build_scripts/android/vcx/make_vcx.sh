#!/bin/bash

set -ex

curl https://sh.rustup.rs -sSf > rustup-init.sh
chmod a+x rustup-init.sh
./rustup-init.sh -y
source .cargo/env
python3 ${ANDROID_NDK_ROOT}/build/tools/make_standalone_toolchain.py --arch ${TARGET_ARCH} --api ${TARGET_API} --install-dir ${TOOLCHAIN_DIR}

cat << EOF > .cargo/config
[target.${CROSS_COMPILE}]
ar = "${AR}"
linker = "${CXX}"
EOF

rustup target add ${CROSS_COMPILE}

cd "${HOME}/aries-vcx/libvcx"
export OPENSSL_STATIC=1
cargo build --release --target=${CROSS_COMPILE}
$CXX -shared -o ${HOME}/libvcx.so -Wl,--whole-archive ${HOME}/aries-vcx/libvcx/target/${CROSS_COMPILE}/release/libvcx.a ${TOOLCHAIN_DIR}/sysroot/usr/lib/${CROSS_COMPILE}/libz.a ${TOOLCHAIN_DIR}/sysroot/usr/lib/${CROSS_COMPILE}/libm.a ${TOOLCHAIN_DIR}/sysroot/usr/lib/${CROSS_COMPILE}/${TARGET_API}/liblog.so ${LIBINDY_DIR}/libindy.a ${OPENSSL_DIR}/lib/libssl.a ${OPENSSL_DIR}/lib/libcrypto.a ${SODIUM_LIB_DIR}/libsodium.a ${LIBZMQ_LIB_DIR}/libzmq.a -Wl,--no-whole-archive -z muldefs
cp "${HOME}/aries-vcx/libvcx/target/${CROSS_COMPILE}/release/libvcx.a" ${HOME}/
