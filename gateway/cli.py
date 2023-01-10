import fire
import json
import requests


class Client:
    url: str

    def __init__(self, url):
        self.url = url

    def new_server(self, name="foo"):
        print(requests.post(f"{self.url}/servers", json={"name": name}).json())

    def new_channel(self, server_id, name="foo"):
        print(
            requests.post(f"{self.url}/channels",
                          json={
                              "name": name,
                              "server_id": server_id
                          }).json())

    def get_server(self, server_id):
        response = requests.get(f"{self.url}/servers/{server_id}")
        json_response = response.json()
        print(json.dumps(json_response))


    def get_channel(self, channel_id):
        response = requests.get(f"{self.url}/channels/{channel_id}")
        json_response = response.json()
        print(json.dumps(json_response))
    
    def clean_voice_servers(self):
        response = requests.post(f"{self.url}/voice_servers/cleanup")
        json_response = response.json()
        print(json.dumps(json_response))

if __name__ == '__main__':
    fire.Fire(Client)
