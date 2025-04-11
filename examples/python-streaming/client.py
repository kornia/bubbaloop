"""Example of a client that requests the streaming image from the server."""

import argparse
import asyncio
import httpx
import rerun as rr
import kornia_rs as kr


async def get_api_response(client: httpx.AsyncClient, url: str) -> dict | None:
    try:
        response = await client.get(url)
    except httpx.HTTPError as _:
        print("The request timed out. Please try again.")
        return

    if response is None:
        return None

    json_response = response.json()
    return json_response


def response_to_image(response: dict) -> rr.Image:
    # decode the image
    decoder = kr.ImageDecoder()
    data = decoder.decode(bytes(response["data"]))
    return rr.Image(data)


async def poll_image(client: httpx.AsyncClient, url: str, rr):
    while True:
        # get the image from the server
        response = await get_api_response(client, url)

        if response is not None and "Success" in response:
            response = response["Success"]
            rr.set_time_sequence("session", response["stamp_ns"])
            rr.log(f"/cam/{response['channel_id']}", response_to_image(response))


async def main() -> None:
    """Main function to receive the streaming image from the server."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=str, default="0.0.0.0")
    parser.add_argument("--port", type=int, default=3000)
    args = parser.parse_args()

    rr.init("rerun_inference_client", spawn=True)

    async with httpx.AsyncClient(timeout=None) as client:
        image_tasks = [
            asyncio.create_task(
                poll_image(
                    client,
                    url=f"http://{args.host}:{args.port}/api/v0/streaming/image/0",
                    rr=rr,
                )
            ),
            asyncio.create_task(
                poll_image(
                    client,
                    url=f"http://{args.host}:{args.port}/api/v0/streaming/image/1",
                    rr=rr,
                )
            ),
        ]
        await asyncio.gather(*image_tasks)


if __name__ == "__main__":
    asyncio.run(main())
