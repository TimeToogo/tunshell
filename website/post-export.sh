#!/bin/bash

# We move the static HTML files to the exact path we want to serve them 
# when loading the website via CloudFront.
# Typically this is handled by using an S3 website origin which does this sort of mapping.
# However, since we are asking people to run scripts on their machines, 
# we want to enforce TLS to the origin server for E2E security.
# S3 websites do not support this: https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/using-https-cloudfront-to-s3-origin.html
# So we use a standard S3 origin and ensure the S3 paths are exactly what is reflected in the final URL.

mv out/go.html out/go
mv out/term.html out/term
mv out/404.html out/404