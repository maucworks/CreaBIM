import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = Path(__file__).resolve().parents[2]
SERVER = Path(__file__).with_name('server.py')
CLI = ROOT / 'target' / 'debug' / ('creabim.exe' if os.name == 'nt' else 'creabim')
SPEC = ROOT / 'examples' / 'small-house.json'


class ProtocolTest(unittest.IsolatedAsyncioTestCase):
    async def test_real_stdio_client_builds_and_inspects_example(self):
        with tempfile.TemporaryDirectory(prefix='creabim mcp ') as temp:
            target = Path(temp) / 'small house.creabim'
            bad_spec = Path(temp) / 'invalid.json'
            bad_spec.write_text('{"version": 1, "walls": [{"id": "bad"}]}')
            warning_spec = Path(temp) / 'warning.json'
            spec_data = json.loads(SPEC.read_text())
            spec_data['openings'][0]['height'] = 20
            warning_spec.write_text(json.dumps(spec_data))
            params = StdioServerParameters(
                command=sys.executable, args=[str(SERVER)],
                env={**os.environ, 'CREABIM_CLI': str(CLI)},
            )
            async with stdio_client(params) as (reader, writer):
                async with ClientSession(reader, writer) as client:
                    await client.initialize()
                    tools = await client.list_tools()
                    names = {t.name for t in tools.tools}
                    self.assertEqual(names, {
                        'inspect_project', 'list_elements', 'get_element',
                        'check_spec', 'build_project', 'get_schema',
                    })
                    result = await client.call_tool('get_schema', {})
                    self.assertFalse(result.isError, result)
                    self.assertIn('$defs', result.structuredContent)
                    result = await client.call_tool('check_spec', {'spec_path': str(warning_spec)})
                    self.assertFalse(result.isError, result)
                    self.assertIn('warning', result.structuredContent['warnings'])
                    result = await client.call_tool('check_spec', {'spec_path': str(SPEC)})
                    self.assertFalse(result.isError, result)
                    result = await client.call_tool('check_spec', {'spec_path': str(bad_spec)})
                    self.assertTrue(result.isError, result)
                    result = await client.call_tool('build_project', {
                        'spec_path': str(SPEC), 'output_path': str(target)})
                    self.assertFalse(result.isError, result)
                    self.assertTrue(target.is_file())
                    original = target.read_bytes()
                    result = await client.call_tool('inspect_project', {'project_path': str(target)})
                    self.assertFalse(result.isError, result)
                    self.assertGreater(result.structuredContent['element_count'], 0)
                    self.assertEqual(result.structuredContent['metadata']['project_name'], 'small house')
                    result = await client.call_tool('list_elements', {
                        'project_path': str(target), 'kind': 'Wall', 'limit': 2})
                    self.assertFalse(result.isError, result)
                    self.assertGreater(result.structuredContent['total'], 2)
                    element = result.structuredContent['elements'][0]
                    result = await client.call_tool('get_element', {
                        'project_path': str(target), 'element_id': element['id']})
                    self.assertFalse(result.isError, result)
                    self.assertEqual(result.structuredContent['id'], element['id'])
                    self.assertIn('Wall', result.structuredContent['kind'])
                    result = await client.call_tool('build_project', {
                        'spec_path': str(SPEC), 'output_path': str(target)})
                    self.assertTrue(result.isError, result)
                    self.assertEqual(original, target.read_bytes())
                    warning_target = Path(temp) / 'with-warning.creabim'
                    result = await client.call_tool('build_project', {
                        'spec_path': str(warning_spec), 'output_path': str(warning_target)})
                    self.assertFalse(result.isError, result)
                    self.assertIn('warning', result.structuredContent['warnings'])
                    sidecar_target = Path(temp) / 'orphan.creabim'
                    sidecar = Path(str(sidecar_target) + '-wal')
                    sidecar.write_bytes(b'preserve')
                    result = await client.call_tool('build_project', {
                        'spec_path': str(SPEC), 'output_path': str(sidecar_target)})
                    self.assertTrue(result.isError, result)
                    self.assertFalse(sidecar_target.exists())
                    self.assertEqual(sidecar.read_bytes(), b'preserve')
                    if os.name != 'nt':
                        dangling = Path(temp) / 'dangling.creabim'
                        dangling.symlink_to(Path(temp) / 'absent.creabim')
                        result = await client.call_tool('build_project', {
                            'spec_path': str(SPEC), 'output_path': str(dangling)})
                        self.assertTrue(result.isError, result)
                        self.assertTrue(dangling.is_symlink())
                    result = await client.call_tool('inspect_project', {
                        'project_path': str(Path(temp) / 'missing.creabim')})
                    self.assertTrue(result.isError, result)
                    self.assertEqual(original, target.read_bytes())

if __name__ == '__main__':
    unittest.main()
