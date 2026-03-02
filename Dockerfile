FROM rust:1.93-alpine

RUN apk add --no-cache rsync fzf gum openssh

WORKDIR /usr/src/rpio
COPY . .

RUN cargo install --path .

ENTRYPOINT [ "rpio" ]
CMD []