"""Regenerate references from executable CLI help and the registered API router."""
from pathlib import Path
import argparse,json,re,subprocess
parser=argparse.ArgumentParser()
parser.add_argument('--check',action='store_true')
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
repo=root.parent
binary=repo/'target/debug/honeycomb'
blocks=[]
def walk(command):
 result=subprocess.run([str(binary),*command,'--help'],check=True,capture_output=True,text=True).stdout.strip()
 result='\n'.join(line.rstrip() for line in result.splitlines())
 title='honeycomb'+(' '+' '.join(command) if command else '')
 blocks.append(f'## {title}\n\n```text\n{result}\n```\n')
 commands=False
 for line in result.splitlines():
  if line=='Commands:': commands=True; continue
  if commands and line and not line.startswith(' '): commands=False
  if commands:
   match=re.match(r'  ([a-z][a-z0-9-]*)(?:\s|$)',line)
   if match and match[1]!='help': walk([*command,match[1]])
walk([])
content='This reference is generated from the actual CLI command parser. Every command supports `--help`. Global flags can be supplied with subcommands. Examples with credentials print sensitive results only when that workflow explicitly returns them.\n\n'+ '\n'.join(blocks)
p=root/'content/cli-reference.md'
if args.check:
 assert p.read_text()==content, 'CLI reference is stale; regenerate it'
else: p.write_text(content)
pages=json.loads((root/'pages.json').read_text())
if not any(p['slug']=='cli-reference' for p in pages):
 pages.insert(next(i for i,p in enumerate(pages) if p['slug']=='configuration')+1,dict(slug='cli-reference',title='CLI command reference',group='Use Honeycomb',description='Every command, argument, default, and flag from honeycomb --help.'))
 (root/'pages.json').write_text(json.dumps(pages,indent=2)+'\n')
def route_arguments(source):
 # Read only each route call, respecting nested layers and quoted path strings.
 for match in re.finditer(r'\.route\(', source):
  begin=match.end()
  depth=1
  quoted=False
  escaped=False
  for index in range(begin,len(source)):
   char=source[index]
   if quoted:
    if escaped: escaped=False
    elif char=='\\': escaped=True
    elif char=='"': quoted=False
   elif char=='"': quoted=True
   elif char=='(': depth+=1
   elif char==')':
    depth-=1
    if depth==0:
     yield source[begin:index]
     break
  else: raise AssertionError('Unclosed router registration')

rows=[]
modules=['api']
seen=set()
for module in modules:
 if module in seen: continue
 seen.add(module)
 source=(repo/f'crates/server/src/{module}.rs').read_text()
 modules.extend(re.findall(r'\.merge\(super::(\w+)::router\(',source))
 constants=dict(re.findall(r'const\s+(\w+):\s*&str\s*=\s*"([^"]+)"',source))
 for part in route_arguments(source):
  route=re.match(r'\s*(?:"([^"]+)"|(\w+))\s*,',part)
  assert route, f'Unrecognized route path in {module}'
  route_path=route[1] or constants.get(route[2])
  assert route_path, f'Unresolved route constant {route[2]} in {module}'
  handlers=re.findall(r'\b(get|post|put|delete|patch)\(([\w:]+)\)',part)
  assert handlers, f'No handlers for {route_path}'
  for method,handler in handlers:
   handler_parts=handler.split('::')
   handler_module=handler_parts[-2] if len(handler_parts)>1 else module
   rows.append(f'| `{method.upper()}` | `{route_path}` | [`{handler_parts[-1]}`](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/crates/server/src/{handler_module}.rs) |')
routes='| Method | Path | Handler |\n| --- | --- | --- |\n'+'\n'.join(rows)+'\n'
r=root/'content/routes.generated.md'
if args.check: assert r.read_text()==routes, 'API route index is stale'
else: r.write_text(routes)
print(f'Verified {len(blocks)} command help pages and {len(rows)} API methods' if args.check else f'Generated {len(blocks)} command help pages and {len(rows)} API methods')
