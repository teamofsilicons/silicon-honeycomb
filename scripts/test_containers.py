#!/usr/bin/env python3
"""Smoke-test production images locally with disposable configuration; no IAM calls."""
import json
import secrets
import socket
import subprocess
import time
import urllib.error
import urllib.request

containers = []
def launch(image, internal, values):
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0)); port=sock.getsockname()[1]
    command=['docker','run','--rm','-d','-p',f'127.0.0.1:{port}:{internal}']
    for key,value in values.items(): command.extend(['-e', f'{key}={value}'])
    container=subprocess.check_output(command+[image],text=True).strip();containers.append(container)
    base=f'http://127.0.0.1:{port}'
    for _ in range(100):
        try:
            urllib.request.urlopen(base+('/health' if internal==8080 else '/api/config'),timeout=1).close()
            return base
        except OSError: time.sleep(.1)
    raise AssertionError(subprocess.check_output(['docker','logs',container],text=True))
try:
    api=launch('honeycomb-backend:local',8080,{'HONEYCOMB_APP_SECRET':secrets.token_hex(32),'HONEYCOMB_ENCRYPTION_KEY':secrets.token_hex(32),'HONEYCOMB_WEBHOOK_SECRET':secrets.token_hex(32),'IAM_BASE_URL':'https://iam.invalid'})
    assert json.load(urllib.request.urlopen(api+'/api/v1/apps'))['total']==0
    try: urllib.request.urlopen(api+'/api/v1/auth/status')
    except urllib.error.HTTPError as error: assert error.code==401
    else: raise AssertionError('Production auth accepted an anonymous user')
    for site in ['library','console']:
        base=launch('honeycomb-web:local',4173,{'WEB_SESSION_KEY':secrets.token_hex(32),'HONEYCOMB_SITE':site,'WEB_ORIGIN':f'https://{site}.example.com','HONEYCOMB_API_URL':'https://backend.example.com'})
        assert json.load(urllib.request.urlopen(base+'/api/config'))['site']==site
        assert json.load(urllib.request.urlopen(base+'/api/session'))=={'authenticated':False}
        response=urllib.request.urlopen(base+'/')
        assert "script-src 'self'" in response.headers['Content-Security-Policy']
        assert '/assets/' in response.read().decode()
    print('PASS: backend migration/startup/catalog/auth gate and both production website modes, static assets, CSP, anonymous sessions')
finally:
    for container in containers: subprocess.run(['docker','stop',container],stdout=subprocess.DEVNULL,check=False)
