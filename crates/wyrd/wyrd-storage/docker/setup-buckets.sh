#!/bin/sh
set -eu

until mc alias set local http://minio:9000 wyrd-test-key wyrd-test-secret >/dev/null 2>&1; do
  sleep 1
done

mc mb -p local/wyrd-storage-test
mc anonymous set none local/wyrd-storage-test

cat > /tmp/lifecycle.json <<'EOF'
{
  "Rules": [
    {
      "ID": "wyrd-abort-incomplete-multipart",
      "Status": "Enabled",
      "Filter": {},
      "AbortIncompleteMultipartUpload": { "DaysAfterInitiation": 1 }
    }
  ]
}
EOF

mc ilm import local/wyrd-storage-test < /tmp/lifecycle.json
