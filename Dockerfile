# Ralph Wiggum Sandbox Container
# This Dockerfile creates an isolated environment for running Cursor

FROM ubuntu:24.04

# Avoid interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install base dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    gnupg \
    lsb-release \
    sudo \
    iptables \
    dnsutils \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (commonly needed for many projects)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Python (commonly needed)
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (commonly needed)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Cursor CLI
# Note: This assumes Cursor provides a CLI installer.
# Update this when Cursor CLI is publicly available.
# For now, we'll create a placeholder that can be updated.
RUN echo '#!/bin/bash\necho "Cursor CLI not yet installed. Please update Dockerfile."' > /usr/local/bin/cursor \
    && chmod +x /usr/local/bin/cursor

# Create workspace directory
WORKDIR /workspace

# Set git configuration for commits
RUN git config --global user.email "ralph@cursor-ralph.local" \
    && git config --global user.name "Ralph Wiggum"

# Default command
CMD ["/bin/bash"]
