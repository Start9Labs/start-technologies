FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install --no-install-recommends -y mokutil nodejs openssl \
    && cp /usr/bin/mokutil /unpatched-mokutil \
    && dpkg-query -W -f='${binary:Package}\n' > /packages.before \
    && cp -a /etc/apt/sources.list.d /apt-sources.before
COPY projects/start-os/build/image-recipe/mokutil /mokutil-source
COPY projects/start-os/build/tests/mokutil-install-test.sh /mokutil-install-test.sh
RUN /mokutil-install-test.sh /mokutil-source/install
COPY projects/start-os/build/image-recipe/setup-user /setup-user
COPY projects/start-os/build/tests/image-user-test.sh /image-user-test.sh
RUN /image-user-test.sh /setup-user
COPY projects/start-os/build/lib/scripts/enable-kiosk /enable-kiosk
COPY projects/start-os/build/tests/image-kiosk-test.sh /image-kiosk-test.sh
RUN bash /image-kiosk-test.sh /enable-kiosk
COPY projects/start-os/build/tests/mokutil-test.mjs /mokutil-test.mjs
ENTRYPOINT ["node"]
