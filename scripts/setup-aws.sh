#!/bin/bash
# Setup script for AWS S3 bucket for checkpoint storage
# Run this once to create the required infrastructure

set -e

# Configuration - modify these
BUCKET_NAME="${CHECKPOINT_BUCKET:-distributed-training-checkpoints}"
REGION="${AWS_REGION:-us-east-1}"

echo "=== Distributed Training Runtime - AWS Setup ==="
echo ""
echo "This script will create:"
echo "  - S3 bucket: $BUCKET_NAME"
echo "  - Lifecycle policy for automatic cleanup"
echo ""

# Check AWS CLI
if ! command -v aws &> /dev/null; then
    echo "Error: AWS CLI not installed"
    echo "Install: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html"
    exit 1
fi

# Check credentials
if ! aws sts get-caller-identity &> /dev/null; then
    echo "Error: AWS credentials not configured"
    echo "Run: aws configure"
    exit 1
fi

echo "AWS Account: $(aws sts get-caller-identity --query Account --output text)"
echo ""

read -p "Continue? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
fi

# Create bucket
echo "Creating S3 bucket..."
if [ "$REGION" = "us-east-1" ]; then
    aws s3api create-bucket --bucket "$BUCKET_NAME" --region "$REGION" 2>/dev/null || true
else
    aws s3api create-bucket --bucket "$BUCKET_NAME" --region "$REGION" \
        --create-bucket-configuration LocationConstraint="$REGION" 2>/dev/null || true
fi

# Enable versioning (for checkpoint recovery)
echo "Enabling versioning..."
aws s3api put-bucket-versioning --bucket "$BUCKET_NAME" \
    --versioning-configuration Status=Enabled

# Set lifecycle policy to clean up old checkpoints
echo "Setting lifecycle policy..."
cat > /tmp/lifecycle.json << EOF
{
    "Rules": [
        {
            "ID": "CleanupOldCheckpoints",
            "Status": "Enabled",
            "Filter": {
                "Prefix": "distributed-training/"
            },
            "NoncurrentVersionExpiration": {
                "NoncurrentDays": 7
            },
            "AbortIncompleteMultipartUpload": {
                "DaysAfterInitiation": 1
            }
        }
    ]
}
EOF

aws s3api put-bucket-lifecycle-configuration --bucket "$BUCKET_NAME" \
    --lifecycle-configuration file:///tmp/lifecycle.json

rm /tmp/lifecycle.json

# Block public access
echo "Blocking public access..."
aws s3api put-public-access-block --bucket "$BUCKET_NAME" \
    --public-access-block-configuration \
    "BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "S3 Bucket: s3://$BUCKET_NAME"
echo ""
echo "Add these to your environment:"
echo ""
echo "  export AWS_REGION=$REGION"
echo "  export CHECKPOINT_BUCKET=$BUCKET_NAME"
echo ""
echo "Or update config/production.toml:"
echo ""
echo "  [storage.s3]"
echo "  bucket = \"$BUCKET_NAME\""
echo "  region = \"$REGION\""
echo ""
