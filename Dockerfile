FROM ubuntu:noble

# ---------------------
# Install dependencies
# ---------------------
RUN apt update
RUN apt upgrade -y 
RUN apt install -y \
     sudo \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    ca-certificates
# ---------------------

# --------
# Cleanup
# --------
RUN rm -rf /var/lib/apt/lists/* 
# --------

# -------------
# Install Rust
# -------------
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
# -------------

# ----------------------------------------
# Copy the source code into the container
# ----------------------------------------
WORKDIR /ollamadex
COPY . .
# ----------------------------------------

# ------------------
# Build source code
# ------------------
RUN cargo build --release
# ------------------

# ----------------------------------
# Run the server on container start
# ----------------------------------
CMD ["sh", "-c", "./target/release/ollamadex --port ${PORT:-3000}"]
# ----------------------------------