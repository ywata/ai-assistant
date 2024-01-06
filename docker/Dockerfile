FROM rust:1.75


RUN apt-get update && apt-get -y install curl

# https://qiita.com/masato/items/012363803f69afe61c25

ENV USERNAME docker
ENV HOME /home/${USERNAME}
ENV PATH $PATH:${HOME}/bin

RUN useradd -m -d ${HOME} -s /bin/bash ${USERNAME} && \
    echo "${USERNAME}:${USERNAME}" | chpasswd && \
    mkdir ${HOME}/.ssh ${HOME}/tmp && \
    chmod 700 ${HOME}/.ssh && \
    chown -R ${USERNAME}:${USERNAME} ${HOME}/.ssh && \
    echo "docker ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers


USER ${USERNAME}

RUN curl -sSL https://dot.net/v1/dotnet-install.sh | bash /dev/stdin --version latest
ENV DOTNET_ROOT=$HOME/.dotnet
ENV PATH=$PATH:$HOME/.dotnet:$HOME/.dotnet/tools

WORKDIR ${HOME}

