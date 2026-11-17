FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install --no-install-recommends -y mokutil nodejs openssl \
    && cp /usr/bin/mokutil /unpatched-mokutil \
    && dpkg-query -W -f='${binary:Package}\n' > /packages.before \
    && cp -a /etc/apt/sources.list.d /apt-sources.before
COPY projects/start-os/build/image-recipe/mokutil /mokutil-source
RUN /mokutil-source/install
COPY projects/start-os/build/tests/mokutil-test.mjs /mokutil-test.mjs
ENTRYPOINT ["node"]
