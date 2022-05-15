import React from "react";
import * as Styled from "./styled";
import { Container } from "../layout";
import { COLOURS } from "../../theme/colours";

export const Donate = (props: any) => {
  return (
    <Styled.Donate {...props}>
      <Container>
        <h4>Find this useful?</h4>
        <Styled.Button
          dangerouslySetInnerHTML={{
            __html: `<style>.bmc-button img{height: 34px !important;width: 35px !important;margin-bottom: 1px !important;box-shadow: none !important;border: none !important;vertical-align: middle !important;}.bmc-button{padding: 7px 15px 7px 10px !important;line-height: 35px !important;height:51px !important;text-decoration: none !important;display:inline-flex !important;color:${COLOURS.WHITE} !important;background-color:${COLOURS.TAN2} !important;border-radius: 8px !important;border: 1px solid transparent !important;font-size: 24px !important;letter-spacing: 0.6px !important;box-shadow: 0px 1px 2px rgba(0,0,0, 0.5) !important;-webkit-box-shadow: 0px 1px 2px 2px rgba(0,0,0, 0.5) !important;margin: 0 auto !important;font-family:'Cookie', cursive !important;-webkit-box-sizing: border-box !important;box-sizing: border-box !important;}.bmc-button:hover, .bmc-button:active, .bmc-button:focus {-webkit-box-shadow: 0px 1px 2px 2px rgba(0, 0, 0, 0.5) !important;text-decoration: none !important;box-shadow: 0px 1px 2px 2px rgba(0,0,0, 0.5) !important;opacity: 0.85 !important;color:${COLOURS.WHITE} !important;}</style><link href="https://fonts.googleapis.com/css?family=Cookie" rel="stylesheet"><a class="bmc-button" target="_blank" href="https://www.buymeacoffee.com/timetoogo"><img class="bmc-logo" style="width: 35px;height: 34px;" id="new-logo" src="https://cdn.buymeacoffee.com/buttons/bmc-new-btn-logo.svg" alt="BMC logo"><span style="margin-left:5px;font-size:24px !important;">Buy me a coffee</span></a>`,
          }}
        ></Styled.Button>
      </Container>
    </Styled.Donate>
  );
};
