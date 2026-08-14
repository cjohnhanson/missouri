FROM mitmproxy/mitmproxy
RUN apt-get update -qq && apt-get install -y -qq iptables wget && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false -d /home/mitmuser -m mitmuser
# Generate CA cert and make it accessible to mitmuser
RUN HOME=/root mitmdump --mode regular -p 18080 -q & sleep 2 && kill $! && wait $! || true
RUN cp -r /root/.mitmproxy /home/mitmuser/.mitmproxy && chown -R mitmuser:mitmuser /home/mitmuser/.mitmproxy
RUN cp /root/.mitmproxy/mitmproxy-ca-cert.pem /usr/local/share/ca-certificates/mitmproxy.crt && update-ca-certificates
